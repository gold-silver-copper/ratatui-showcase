// Post-processing: bloom (bright pass, then a separable blur at a quarter of
// the resolution), then one pass for feedback trails, the zoom echo,
// chromatic aberration, a CRT's curve and glow, and film grain.

const P_CLEAN: u32 = 0u;
const P_BLOOM: u32 = 1u;
const P_TRAILS: u32 = 2u;
const P_ECHO: u32 = 3u;
const P_CHROMA: u32 = 4u;
const P_CRT: u32 = 5u;
const P_GRAIN: u32 = 6u;

@compute @workgroup_size(64)
fn bloom_down(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= bloom_w() * bloom_h() { return; }
    let bx = i % bloom_w();
    let by = i / bloom_w();
    var sum = vec3(0.0);
    for (var y = 0u; y < 4u; y++) {
        for (var x = 0u; x < 4u; x++) {
            sum += state[params[H_SCENE] + img_index(bx * 4u + x, by * 4u + y)].rgb;
        }
    }
    let c = sum / 16.0;
    let bright = max(c - vec3(0.5), vec3(0.0)) * 1.6;
    state[params[H_BLOOM0] + i] = vec4(bright, 1.0);
}

fn blur(src: u32, dst: u32, i: u32, along: vec2<i32>) {
    let bx = i32(i % bloom_w());
    let by = i32(i / bloom_w());
    let weights = array<f32, 5>(0.227, 0.194, 0.121, 0.054, 0.016);
    var sum = bloom_load(src, bx, by).rgb * weights[0];
    for (var k = 1; k < 5; k++) {
        let w = weights[k];
        sum += (bloom_load(src, bx + along.x * k, by + along.y * k).rgb + bloom_load(src, bx - along.x * k, by - along.y * k).rgb) * w;
    }
    state[params[dst] + i] = vec4(sum, 1.0);
}

@compute @workgroup_size(64)
fn bloom_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= bloom_w() * bloom_h() { return; }
    blur(H_BLOOM0, H_BLOOM1, id.x, vec2(2, 0));
}

@compute @workgroup_size(64)
fn bloom_vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= bloom_w() * bloom_h() { return; }
    blur(H_BLOOM1, H_BLOOM0, id.x, vec2(0, 1));
}

fn bloom_sample(p: vec2<f32>) -> vec3<f32> {
    let b = p / 4.0 - 0.5;
    let i = vec2<i32>(floor(b));
    let f = b - floor(b);
    let a = bloom_load(H_BLOOM0, i.x, i.y).rgb;
    let c = bloom_load(H_BLOOM0, i.x + 1, i.y).rgb;
    let d = bloom_load(H_BLOOM0, i.x, i.y + 1).rgb;
    let e = bloom_load(H_BLOOM0, i.x + 1, i.y + 1).rgb;
    return mix(mix(a, c, f.x), mix(d, e, f.x), f.y);
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

    var c = img_load(H_SCENE, p).rgb;
    if effect == P_CHROMA {
        let shift = (p - centre) / size * (6.0 + level() * 10.0);
        c = vec3(img_load(H_SCENE, p + shift).r, c.g, img_load(H_SCENE, p - shift).b);
    }
    let strength = select(0.35, 1.0, effect == P_BLOOM) + level() * 0.8;
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
