// Post-processing: bloom at three sizes (bright pass, halving, separable
// blurs), then one pass for depth of field, feedback trails, the zoom echo,
// chromatic aberration, a CRT's curve and glow, and film grain.

const P_CLEAN: u32 = 0u;
const P_BLOOM: u32 = 1u;
const P_TRAILS: u32 = 2u;
const P_ECHO: u32 = 3u;
const P_CHROMA: u32 = 4u;
const P_CRT: u32 = 5u;
const P_GRAIN: u32 = 6u;

/// The two buffers of bloom `level`.
fn bloom_a(level: u32) -> u32 { return select(select(H_BLOOM4, H_BLOOM2, level == 1u), H_BLOOM0, level == 0u); }
fn bloom_b(level: u32) -> u32 { return select(select(H_BLOOM5, H_BLOOM3, level == 1u), H_BLOOM1, level == 0u); }

/// Level STEP of the bloom chain: the scene's bright parts, at a quarter of
/// the resolution, then each level half the one before.
@compute @workgroup_size(64)
fn bloom_down(@builtin(global_invocation_id) id: vec3<u32>) {
    let level = STEP;
    let i = id.x;
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
fn bloom_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    let level = STEP;
    let i = id.x;
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
fn post(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
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
            state[params[H_FINAL] + i] = vec4(0.0, 0.0, 0.0, 1.0);
            return;
        }
    }

    var c = focused(p);
    if effect == P_CHROMA {
        let shift = (p - centre) / size * 6.0;
        c = vec3(img_load(H_SCENE, p + shift).r, c.g, img_load(H_SCENE, p - shift).b);
    }
    let strength = select(0.35, 1.0, effect == P_BLOOM);
    c += bloom_sample(p) * strength;

    // Feedback from the previous frame.
    if effect == P_TRAILS {
        c = max(c, img_load(H_PREVIOUS, p).rgb * 0.88);
    } else if effect == P_ECHO {
        let q = centre + rot(0.012) * (p - centre) * 0.975;
        c = max(c, img_load(H_PREVIOUS, q).rgb * 0.9);
    } else if effect == P_CRT {
        c = max(c, img_load(H_PREVIOUS, p).rgb * 0.55);
        c *= select(1.0, 0.78, sy % 2u == 1u);
        c *= 1.0 - 0.25 * dot(uv, uv);
    }
    if effect == P_GRAIN {
        c += vec3((hash3(sx, sy, frame()) - 0.5) * 0.09);
        c *= 1.0 - 0.3 * dot(uv * 0.7, uv * 0.7);
    }
    state[params[H_FINAL] + i] = vec4(max(c, vec3(0.0)), 1.0);
}
