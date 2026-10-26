// The scene pass: every sample of the frame, before post-processing.

/// Scene `s` (as `layer`) at `(x, y)`, drawn the way its mode says: a
/// simulation from its buffers while it runs, else as it last looked.
fn layer_art(s: u32, layer: u32, x: f32, y: f32, t: f32, row: u32) -> Art {
    let m = mode(s);
    if m == M_TURNTABLE { return turntable(s, layer, x, y, t, row); }
    let simulated = m == M_FLUID || m == M_REACTION || m == M_LIFE || m == M_FIRE || m == M_SLIME || m == M_SAND
        || m == M_CRYSTAL || m == M_INK || m == M_LENIA;
    // Far from flat art there is nothing to draw: not even glow or shadow.
    // Art too fine for the field to find any edge is drawn regardless.
    let reach = max(pf(s + S_GLOW), 1.0) * 6.0 + max(pf(s + S_WIDTH), 1.0) * 3.0;
    let d = sdf(layer, x, y);
    if !simulated && d > reach && d < FAR * 0.5 { return Art(vec3(0.0), 0.0, vec3(0.0), 0.0, 0.0); }
    if simulated {
        if params[H_SIM_SCENE] == s { return sim_art(s, layer, x, y, t, row, sim_latest()); }
        return frozen(x, y);
    }
    if m == M_SWARM || m == M_BOIDS {
        var ghost = art(s, layer, x, y, t, row);
        ghost.alpha *= 0.1;
        ghost.glow *= 0.5;
        return ghost;
    }
    return art(s, layer, x, y, t, row);
}

/// How far water bends the view at `p`: the new scene's, or the old one's
/// as it goes.
fn water(p: vec2<f32>) -> vec2<f32> {
    let sim = params[H_SIM_SCENE];
    let fading = 1.0 - progress();
    if sim != 0u && mode(sim) == M_RIPPLE {
        return ripple_offset(sim_latest(), p) * select(1.0, fading, sim == H_FROM && kind() != K_STILL);
    }
    if kind() != K_STILL && mode(H_FROM) == M_RIPPLE { return ripple_offset(H_SNAPSHOT, p) * fading; }
    return vec2(0.0);
}

/// Relative luminance: colours here are about the square root of linear
/// light (see `mixl`).
fn luminance(c: vec3<f32>) -> f32 { return dot(c * c, vec3(0.2126, 0.7152, 0.0722)); }

/// The contrast the art keeps against what is behind and around it, as a
/// ratio of luminances: WCAG's minimum for large text.
const CONTRAST: f32 = 3.0;

/// `bg`, behind or around scene `s`'s art `d` samples from its edge,
/// dimmed just as far as the art's ink needs to stand out from it by
/// `CONTRAST`: a dark background is left alone, a bright fractal gets a
/// dark halo. The post pass checks again after bloom and the rest.
fn legible(s: u32, d: f32, bg: vec3<f32>) -> vec3<f32> {
    let reach = max(pf(s + S_GLOW), 1.0) * 3.0;
    if d > reach { return bg; }
    let n = clamp(params[s + S_STOPS], 1u, 5u);
    var ink = 0.0;
    for (var i = 0u; i < n; i++) { ink += luminance(rgb3(s + S_STOP0 + i * 3u)); }
    let ceiling = max((ink / f32(n) + 0.05) / CONTRAST - 0.05, 0.003);
    let l = luminance(bg);
    if l <= ceiling { return bg; }
    return bg * mix(sqrt(ceiling / l), 1.0, smoothstep(0.0, reach, d));
}

@compute @workgroup_size(64)
fn scene(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= img_w() * img_h() { return; }
    let sx = i % img_w();
    let sy = i / img_w();
    var p = vec2(f32(sx) + 0.5, f32(sy) + 0.5);
    let t = time();
    let k = kind();
    let row = sy / 4u;

    // Water bends everything under it.
    var glint = 0.0;
    let o = water(p);
    if any(o != vec2(0.0)) {
        p += o * 5.0;
        let n = normalize(vec3(-o * 1.2, 1.0));
        glint = pow(max(dot(n, normalize(vec3(-0.4, -0.6, 0.7))), 0.0), 90.0) * 0.5;
    }
    let x01 = p.x / width();
    let y01 = p.y / height();

    // Backgrounds, mixed between the scenes, from the grid the backdrop pass
    // drew; flames and attractors come from their histogram.
    let m = backdrop_mix(x01, y01);
    var base: vec3<f32>;
    if k == K_CRT {
        let bg = select(H_BG_NEW, H_BG_OLD, progress() < 0.5);
        base = select(vec3(0.0), grid_sample(bg, p).rgb * (1.0 - crt_q()), warp(0u, p.x, p.y).z > 0.0);
    } else {
        var bg_new = vec3(0.0);
        var bg_old = vec3(0.0);
        if m > 0.0 {
            bg_new = grid_sample(H_BG_NEW, p).rgb;
            if splatted(H_TO) { bg_new += splat_light(H_TO, p.x, p.y); }
            bg_new = legible(H_TO, sdf(1u, p.x, p.y), bg_new);
        }
        if m < 1.0 {
            bg_old = grid_sample(H_BG_OLD, p).rgb;
            if splatted(H_FROM) && !splatted(H_TO) { bg_old += splat_light(H_FROM, p.x, p.y); }
            bg_old = legible(H_FROM, sdf(0u, p.x, p.y), bg_old);

        }
        base = mixl(bg_old, bg_new, m);
    }
    base += light(p.x, p.y);

    var ink = vec3(0.0);
    var alpha = 0.0;
    var shadow = 0.0;
    var blur = 0.0;
    if k == K_MELT {
        // The old shape's distance field flows into the new one's.
        let e = ease_in_out(progress());
        let wobble = (fbm(p * 0.03 + vec2(t * 0.6, 0.0)) - 0.5) * 8.0 * sin(e * PI);
        let d = mix(sdf(0u, p.x, p.y), sdf(1u, p.x, p.y), e) + wobble;
        let a = art(H_FROM, 0u, p.x, p.y, t, row);
        let b = art(H_TO, 1u, p.x, p.y, t, row);
        ink = mixl(a.color, b.color, e);
        alpha = clamp(0.5 - d, 0.0, 1.0);
        let radius = mix(pf(H_FROM + S_GLOW), pf(H_TO + S_GLOW), e);
        base += ink * exp(-max(d, 0.0) / max(radius, 1.0)) * select(0.0, 0.35, d > 0.0);
    } else {
        var weight = 0.0;
        for (var layer = 0u; layer < 2u; layer++) {
            if k == K_STILL && layer == 0u { continue; }
            let s = select(H_FROM, H_TO, layer == 1u);
            let op = opacity(layer, p.x, p.y);
            if op <= 0.0 { continue; }
            let wp = warp(layer, p.x, p.y);
            if wp.z <= 0.0 { continue; }
            let a = layer_art(s, layer, wp.x, wp.y, t, row);
            var c = a.color;
            if k == K_CRT { c = lerp(c, WHITE, crt_q() * 0.8); }
            ink += c * a.alpha * op;
            weight += a.alpha * op;
            base += a.glow * op;
            shadow = max(shadow, a.shadow * op);
            blur = max(blur, a.blur * op);
        }
        if weight > 0.0 { ink /= weight; }
        alpha = min(weight, 1.0);
    }

    // Transitions that draw over both scenes.
    if k == K_INFERNO {
        let cell = grid_sample(sim_latest(), p);
        let flame = clamp(cell.x * 1.8, 0.0, 1.0);
        let fire = fire_color(H_TO, cell.x);
        ink = mixl(ink, fire, flame / max(alpha + flame, 1e-3));
        alpha = max(alpha, flame);
        base += fire * cell.x * 0.3 + vec3(clamp(cell.y, 0.0, 0.6) * 0.15);
    } else if k == K_DISSOLVE {
        let dye = grid_sample(H_SIM2, p).rgb;
        let strength = max(dye.r, max(dye.g, dye.b));
        let a = clamp(strength * 1.6, 0.0, 1.0) * (1.0 - smoothstep(0.55, 1.0, progress()));
        ink = mixl(ink, dye / max(strength, 1e-3) * min(strength * 1.3, 1.0), a / max(alpha + a, 1e-3));
        alpha = max(alpha, a);
    } else if k == K_PAGE_CURL {
        let c = curl(p);
        if c.z == 1.0 {
            // The back of the page: paper, the print showing through, darker
            // toward the fold.
            ink = mixl(ink * 0.35 + vec3(0.62, 0.6, 0.56), WHITE * 0.9, 0.2) * (0.6 + 0.4 * c.w);
            alpha = 1.0;
        } else if c.z == 0.0 {
            base *= 1.0 - 0.55 * (1.0 - smoothstep(0.0, 0.8, c.w));
        }
    }

    // Particles splatted this frame.
    if is_particles(k) || params[H_SWARM] > 0u {
        let cell = (sy / ROWS) * params[H_COLS] + sx / COLS;
        let a = f32(atomicLoad(&cover[cell * SAMPLES + (sy % ROWS) * COLS + sx % COLS])) / 255.0;
        if a > 0.0 {
            let slot = (cell * 2u + select(0u, 1u, sy % ROWS >= ROWS / 2u)) * 4u;
            let w = f32(atomicLoad(&color[slot + 3u]));
            if w > 0.0 {
                let c = vec3(f32(atomicLoad(&color[slot])), f32(atomicLoad(&color[slot + 1u])), f32(atomicLoad(&color[slot + 2u]))) / w;
                ink = mixl(ink, c, a / max(alpha + a, 1e-3));
                alpha = min(alpha + a, 1.0);
            }
        }
    }

    base *= 1.0 - shadow;
    let colour = mixl(base, ink, alpha) + WHITE * glint;
    // Alpha carries the depth-of-field blur radius to the post pass.
    state[params[H_SCENE] + i] = vec4(colour, blur);
}
