// Post-processing: bloom at three sizes (bright pass, halving, separable
// blurs; skipped for clean frames), then one pass for depth of field,
// feedback trails, the zoom echo, chromatic aberration, a CRT's curve and
// glow, film grain and light shafts.

/// The two buffers of bloom `level`.
fn bloom_a(level: u32) -> u32 { return select(select(H_BLOOM4, H_BLOOM2, level == 1u), H_BLOOM0, level == 0u); }
fn bloom_b(level: u32) -> u32 { return select(select(H_BLOOM5, H_BLOOM3, level == 1u), H_BLOOM1, level == 0u); }

/// Level STEP of the bloom chain: the scene's bright parts, at a quarter of
/// the resolution, then each level half the one before.
@compute @workgroup_size(64)
fn bloom_down(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let level = STEP;
    let i = thread(id, groups);
    if i >= bloom_w(level) * bloom_h(level) { return; }
    let bx = i % bloom_w(level);
    let by = i / bloom_w(level);
    var c = vec3(0.0);
    if level == 0u {
        var sum = vec3(0.0);
        for (var y = 0u; y < 4u; y++) {
            for (var x = 0u; x < 4u; x++) {
                sum += state[params[H_SCENE] + img_index(min(bx * 4u + x, img_w() - 1u), min(by * 4u + y, img_h() - 1u))].rgb;
            }
        }
        c = max(sum / 16.0 - vec3(0.5), vec3(0.0)) * 1.6;
    } else {
        let src = bloom_a(level - 1u);
        let x = i32(bx * 2u);
        let y = i32(by * 2u);
        c = (bloom_load(src, level - 1u, x, y).rgb + bloom_load(src, level - 1u, x + 1, y).rgb
            + bloom_load(src, level - 1u, x, y + 1).rgb + bloom_load(src, level - 1u, x + 1, y + 1).rgb) / 4.0;
    }
    state[params[bloom_a(level)] + i] = vec4(c, 1.0);
}

/// Blurs bloom level STEP: across into its second buffer (FLIP 0), then
/// down back into its first (FLIP 1).
@compute @workgroup_size(64)
fn bloom_blur(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let level = STEP;
    let i = thread(id, groups);
    if i >= bloom_w(level) * bloom_h(level) { return; }
    let src = select(bloom_a(level), bloom_b(level), FLIP == 1u);
    let dst = select(bloom_b(level), bloom_a(level), FLIP == 1u);
    let along = select(vec2(1, 0), vec2(0, 1), FLIP == 1u);
    let bx = i32(i % bloom_w(level));
    let by = i32(i / bloom_w(level));
    // A variable, not a constant: once the pipeline's constants are filled
    // in, naga folds a constant array indexed in a loop out of scope.
    var weights = array<f32, 5>(0.227, 0.194, 0.121, 0.054, 0.016);
    var sum = bloom_load(src, level, bx, by).rgb * weights[0];
    for (var k = 1; k < 5; k++) {
        sum += (bloom_load(src, level, bx + along.x * k, by + along.y * k).rgb
            + bloom_load(src, level, bx - along.x * k, by - along.y * k).rgb) * weights[k];
    }
    state[params[dst] + i] = vec4(sum, 1.0);
}

fn bloom_level(level: u32, p: vec2<f32>) -> vec3<f32> {
    let size = f32(4u << level);
    let b = p / size - 0.5;
    let i = vec2<i32>(floor(b));
    let f = b - floor(b);
    let base = bloom_a(level);
    let a = bloom_load(base, level, i.x, i.y).rgb;
    let c = bloom_load(base, level, i.x + 1, i.y).rgb;
    let d = bloom_load(base, level, i.x, i.y + 1).rgb;
    let e = bloom_load(base, level, i.x + 1, i.y + 1).rgb;
    return mix(mix(a, c, f.x), mix(d, e, f.x), f.y);
}

/// Glow at three sizes: tight, medium and wide.
fn bloom_sample(p: vec2<f32>) -> vec3<f32> {
    return bloom_level(0u, p) + bloom_level(1u, p) * 0.8 + bloom_level(2u, p) * 0.6;
}

/// Light shafts: the bright parts of the frame smeared out from the art's
/// centre, as if it shone through haze.
fn rays(p: vec2<f32>) -> vec3<f32> {
    let centre = art_bounds(H_TO).centre;
    let step = (centre - p) * 0.045;
    var q = p;
    var weight = 1.0;
    var total = 0.0;
    var sum = vec3(0.0);
    for (var k = 0; k < 16; k++) {
        q += step;
        sum += bloom_level(0u, q) * weight;
        total += weight;
        weight *= 0.9;
    }
    return sum / total * 0.6;
}

/// How much of the last frame survives this one, from how much survives a
/// sixtieth of a second, so feedback fades as fast at any frame rate.
fn persist(per_tick: f32) -> f32 { return pow(per_tick, clamp(pf(H_DT), 0.0, 0.1) * 60.0); }

fn scene_luma(p: vec2<f32>) -> f32 { return luma(img_load(H_SCENE, p).rgb); }

/// The direction the picture changes fastest at `p` (x, y) and how strongly
/// one direction dominates (z), from the structure tensor of its brightness.
fn structure(p: vec2<f32>) -> vec3<f32> {
    var t = vec3(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let q = p + vec2(f32(dx), f32(dy)) * 2.0;
            let g = vec2(scene_luma(q + vec2(2.0, 0.0)) - scene_luma(q - vec2(2.0, 0.0)),
                scene_luma(q + vec2(0.0, 2.0)) - scene_luma(q - vec2(0.0, 2.0)));
            t += vec3(g.x * g.x, g.y * g.y, g.x * g.y);
        }
    }
    let angle = 0.5 * atan2(2.0 * t.z, t.x - t.y);
    let spread = sqrt(max((t.x - t.y) * (t.x - t.y) + 4.0 * t.z * t.z, 0.0));
    return vec3(cos(angle), sin(angle), spread / max(t.x + t.y, 1e-5));
}

/// Oil paint: an anisotropic Kuwahara filter (Kyprianidis, Kang and Dollner,
/// 2009). Around each point, an ellipse stretched along the edges is cut
/// into four sectors; the sectors that vary least carry most weight, so
/// strokes flatten colour along edges and never across them.
fn paint(p: vec2<f32>) -> vec3<f32> {
    let st = structure(p);
    let across = st.xy;
    let along = vec2(-across.y, across.x);
    let radius = 10.0;
    let stretch = 1.0 + st.z * 1.5;
    var total = vec3(0.0);
    var weights = 0.0;
    for (var j = 0; j < 4; j++) {
        var sum = vec3(0.0);
        var sq = vec3(0.0);
        for (var k = 0; k < 9; k++) {
            let a = (f32(j) + (f32(k % 3) + 0.5) / 3.0) * PI * 0.5;
            let r = radius * (f32(k / 3) + 1.0) / 3.0;
            let q = p + (along * cos(a) * stretch + across * sin(a) / stretch) * r;
            let c = img_load(H_SCENE, q).rgb;
            sum += c;
            sq += c * c;
        }
        let mean = sum / 9.0;
        let variance = sq / 9.0 - mean * mean;
        let w = 1.0 / pow(1e-3 + (variance.r + variance.g + variance.b) * 40.0, 4.0);
        total += mean * w;
        weights += w;
    }
    return total / max(weights, 1e-9);
}

/// The flow at `p` for silk: the fluid's own velocity when one runs, else
/// along the picture's edges, else slow eddies.
fn flow(p: vec2<f32>) -> vec2<f32> {
    let sim = params[H_SIM_SCENE];
    if sim != 0u && (mode(sim) == M_FLUID || mode(sim) == M_INK) {
        let v = grid_sample(H_SIM0, p).xy;
        if dot(v, v) > 1e-4 { return v; }
    }
    let g = vec2(scene_luma(p + vec2(2.0, 0.0)) - scene_luma(p - vec2(2.0, 0.0)),
        scene_luma(p + vec2(0.0, 2.0)) - scene_luma(p - vec2(0.0, 2.0)));
    if dot(g, g) > 1e-4 { return vec2(-g.y, g.x); }
    let n = fbm(p * 0.02 + vec2(time() * 0.05, 0.0)) * TAU * 2.0;
    return vec2(cos(n), sin(n));
}

/// Silk: line integral convolution (Cabral and Leedom, 1993). Each point
/// averages noise and colour along the flow line through it, both ways, so
/// the flow shows as fine streaks; the noise's phase drifts so they run.
fn silk(p: vec2<f32>) -> vec3<f32> {
    var noise = 0.0;
    var colour = vec3(0.0);
    var n = 0.0;
    for (var side = -1.0; side <= 1.0; side += 2.0) {
        var q = p;
        for (var k = 0; k < 14; k++) {
            let v = flow(q);
            q += normalize(v) * side * 2.0;
            // Noise in blocks of 3 samples: finer would be averaged away
            // inside each cell.
            let h = hash(u32(max(q.x, 0.0) / 3.0), u32(max(q.y, 0.0) / 3.0) * 3u + 1u);
            noise += 0.5 + 0.5 * sin(h * TAU + time() * 4.0 - f32(k) * side * 0.6);
            colour += img_load(H_SCENE, q).rgb;
            n += 1.0;
        }
    }
    return colour / n * (0.25 + 1.5 * noise / n);
}

/// The Droste effect (after de Smit and Lenstra's reading of Escher's Print
/// Gallery): the picture's outer ring repeats inside itself along a log
/// spiral, each turn a copy smaller by `k`, zooming in for ever.
fn droste(p: vec2<f32>) -> vec2<f32> {
    let centre = art_bounds(H_TO).centre;
    let unit = height();
    let z = (p - centre) / unit;
    let k = 2.6;
    let lk = log(k);
    let outer = log(0.5 * length(vec2(width(), height())) / unit);
    let low = outer - lk;
    let theta = atan2(z.y, z.x);
    let twist = lk / TAU;
    let rho = log(max(length(z), 1e-5)) + theta * twist;
    let wrapped = low + wrap((rho - low + time() * 0.12) / lk) * lk - theta * twist;
    return centre + exp(wrapped) * vec2(cos(theta), sin(theta)) * unit;
}

/// Depth of field: a disc blur as wide as the scene asked for.
fn focused(p: vec2<f32>) -> vec3<f32> {
    let here = img_load(H_SCENE, p);
    let radius = here.a;
    if radius < 0.3 { return here.rgb; }
    var sum = here.rgb;
    for (var k = 0; k < 12; k++) {
        let angle = f32(k) * 2.39996;
        let reach = radius * sqrt((f32(k) + 0.5) / 12.0) * 2.0;
        sum += img_load(H_SCENE, p + vec2(cos(angle), sin(angle)) * reach).rgb;
    }
    return sum / 13.0;
}

@compute @workgroup_size(64)
fn post(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= img_w() * img_h() { return; }
    let sx = i % img_w();
    let sy = i / img_w();
    let size = vec2(width(), height());
    let centre = size / 2.0;
    let effect = params[H_POST];
    var p = vec2(f32(sx) + 0.5, f32(sy) + 0.5);

    // A CRT's curved glass.
    var uv = p / size * 2.0 - 1.0;
    if effect == P_CRT {
        uv *= 1.0 + 0.07 * dot(uv, uv);
        p = (uv + 1.0) / 2.0 * size;
        if abs(uv.x) > 1.0 || abs(uv.y) > 1.0 {
            frames[final_index(0u, vec2(f32(sx), f32(sy)))] = 0u;
            return;
        }
    }

    if effect == P_DROSTE { p = droste(p); }
    var c = focused(p);
    if effect == P_PAINT { c = paint(p); }
    if effect == P_SILK { c = silk(p); }
    if effect == P_CHROMA {
        let shift = (p - centre) / size * 6.0;
        c = vec3(img_load(H_SCENE, p + shift).r, c.g, img_load(H_SCENE, p - shift).b);
    }
    // Clean frames have no bloom: its passes didn't run.
    if effect != P_CLEAN {
        c += bloom_sample(p) * select(0.35, 1.0, effect == P_BLOOM);
    }
    if effect == P_RAYS {
        c += rays(p);
    }

    // Feedback from the previous frame. Scanlines and grain change per half
    // cell, the finest detail the characters keep.
    if effect == P_TRAILS {
        c = max(c, final_load(1u, p) * persist(0.88));
    } else if effect == P_ECHO {
        let q = centre + rot(0.012) * (p - centre) * 0.975;
        c = max(c, final_load(1u, q) * persist(0.9));
    } else if effect == P_CRT {
        c = max(c, final_load(1u, p) * persist(0.55));
        c *= select(1.0, 0.72, (sy / 4u) % 2u == 1u);
        c *= 1.0 - 0.25 * dot(uv, uv);
    }
    if effect == P_GRAIN {
        c += vec3((hash3(sx / 4u, sy / 4u, frame()) - 0.5) * 0.12);
        c *= 1.0 - 0.3 * dot(uv * 0.7, uv * 0.7);
    }
    frames[final_index(0u, vec2(f32(sx), f32(sy)))] = pack4x8unorm(vec4(clamp(c, vec3(0.0), vec3(1.0)), 1.0));
}
