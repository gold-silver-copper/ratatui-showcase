// Shared by every pass: bindings, small helpers, and how passes keep state
// between frames. The params layout and every option's number (H_*, S_*,
// B_*, K_* and so on) are generated from the Rust side (shader.rs).
//
// Positions are in samples: each terminal cell is 4 x 8 of them. Images in
// `state` are one vec4 per sample; simulations, distance fields and
// backgrounds run on a grid of 2 x 2 samples ("grid"); bloom on 4 x 4 and
// coarser.

@group(0) @binding(0) var<storage, read> params: array<u32>;
@group(0) @binding(1) var<storage, read> inputs: array<u32>;
@group(0) @binding(2) var<storage, read_write> state: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;
@group(0) @binding(4) var<storage, read_write> cover: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> color: array<atomic<u32>>;
// Flames and attractors: hits and summed colour index per sample.
@group(0) @binding(6) var<storage, read_write> hist: array<atomic<u32>>;
// The final image of this frame and of the last, packed to 8 bits a channel.
@group(0) @binding(7) var<storage, read_write> frames: array<u32>;
// Particles counted per 16 x 16 samples: for each of particle life's four
// kinds a count and summed position, then the boids' count, summed
// velocity and position. Positions are summed in half samples, velocities
// as signed fixed point.
@group(0) @binding(8) var<storage, read_write> bins: array<atomic<u32>>;
const BIN: f32 = 16.0;
const BIN_WORDS: u32 = 17u;
const BIN_BOIDS: u32 = 12u;

fn bins_w() -> u32 { return (img_w() + 15u) / 16u; }
fn bins_h() -> u32 { return (img_h() + 15u) / 16u; }
/// The first word of bin `(bx, by)`, clamped to the grid of bins.
fn bin_at(bx: i32, by: i32) -> u32 {
    let x = u32(clamp(bx, 0, i32(bins_w()) - 1));
    let y = u32(clamp(by, 0, i32(bins_h()) - 1));
    return (y * bins_w() + x) * BIN_WORDS;
}
fn bin_of(p: vec2<f32>) -> vec2<i32> { return vec2<i32>(floor(p / BIN)); }

// Pipeline constants: jump size for distance fields or bloom level, and
// which of two buffers a ping-pong pass reads.
override STEP: u32 = 1u;
override FLIP: u32 = 0u;

/// Which thread this is: big passes spread their workgroups over two axes.
fn thread(id: vec3<u32>, groups: vec3<u32>) -> u32 { return id.x + id.y * groups.x * 64u; }

// Marks: words after the cells in `output` that the GPU keeps for itself
// between frames, updated by the last pass (`finish`). They decide what
// must stay in step with the GPU's own state, so a frame that never ran
// can't put it out of step.
/// The scene the simulation buffers belong to.
const MARK_SIM: u32 = 0u;
/// Simulation steps so far: which ping-pong buffer is current.
const MARK_PARITY: u32 = 1u;
/// Frames drawn so far: which final image is this frame's.
const MARK_FRAME: u32 = 2u;
/// The splatted backdrop the flame points belong to.
const MARK_SPLAT: u32 = 3u;
/// The scene the distance fields were built for, if they can be kept.
const MARK_FIELD: u32 = 4u;

fn marks() -> u32 { return 2u + params[H_COLS] * params[H_ROWS] * 3u; }
fn mark(k: u32) -> u32 { return output[marks() + k]; }

const COLS: u32 = 4u;
const ROWS: u32 = 8u;
const SAMPLES: u32 = 32u;
const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;
const WHITE: vec3<f32> = vec3<f32>(1.0);
const FAR: f32 = 1e5;

fn pf(i: u32) -> f32 { return bitcast<f32>(params[i]); }
fn time() -> f32 { return pf(H_TIME); }
fn progress() -> f32 { return pf(H_PROGRESS); }
fn kind() -> u32 { return params[H_KIND]; }
fn frame() -> u32 { return params[H_FRAME]; }
fn dt() -> f32 { return clamp(pf(H_DT), 0.001, 0.05); }

/// Image size in samples.
fn img_w() -> u32 { return params[H_COLS] * COLS; }
fn img_h() -> u32 { return params[H_ROWS] * ROWS; }
fn width() -> f32 { return f32(img_w()); }
fn height() -> f32 { return f32(img_h()); }

/// Grid size: 2 x 2 samples per grid point.
fn grid_w() -> u32 { return params[H_COLS] * 2u; }
fn grid_h() -> u32 { return params[H_ROWS] * 4u; }

/// Bloom size at `level`: 4 x 4 samples per point at level 0, halving after.
fn bloom_w(level: u32) -> u32 { return max(params[H_COLS] >> level, 1u); }
fn bloom_h(level: u32) -> u32 { return max((params[H_ROWS] * 2u) >> level, 1u); }

fn mode(s: u32) -> u32 { return params[s + S_MODE]; }
fn variant(s: u32) -> u32 { return params[s + S_VARIANT]; }
fn style(s: u32) -> u32 { return params[s + S_STYLE]; }
fn age(s: u32) -> f32 { return pf(s + S_AGE); }

fn hash(x: u32, y: u32) -> f32 {
    var h = (x * 0x9e3779b1u) ^ (y * 0x85ebca77u);
    h ^= h >> 15u;
    h *= 0x2c1b3c6du;
    h ^= h >> 12u;
    return f32(h & 0xffffffu) / 16777216.0;
}

fn hash3(x: u32, y: u32, z: u32) -> f32 { return hash(x ^ (z * 0x27d4eb2du), y + z * 0x165667b1u); }
fn hashf(x: f32, y: f32) -> f32 { return hash(u32(max(x, 0.0)), u32(max(y, 0.0))); }

fn wrap(x: f32) -> f32 { return x - floor(x); }
fn smoothed(e0: f32, e1: f32, x: f32) -> f32 { return smoothstep(e0, e1, x); }
fn lerp(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> { return mix(a, b, clamp(t, 0.0, 1.0)); }

/// Blends in linear light.
fn mixl(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> {
    return sqrt(max(mix(a * a, b * b, clamp(t, 0.0, 1.0)), vec3(0.0)));
}

fn luma(c: vec3<f32>) -> f32 { return dot(c, vec3(0.3, 0.59, 0.11)); }

fn hsl(h: f32, s: f32, l: f32) -> vec3<f32> {
    let a = s * min(l, 1.0 - l);
    let k = (vec3(0.0, 8.0, 4.0) + wrap(h) * 12.0) % vec3(12.0);
    return l - a * clamp(min(k - 3.0, 9.0 - k), vec3(-1.0), vec3(1.0));
}

fn hex(h: u32) -> vec3<f32> {
    return vec3(f32((h >> 16u) & 0xffu), f32((h >> 8u) & 0xffu), f32(h & 0xffu)) / 255.0;
}

fn rot(a: f32) -> mat2x2<f32> { return mat2x2(cos(a), sin(a), -sin(a), cos(a)); }

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = p - i;
    let s = f * f * (3.0 - 2.0 * f);
    let a = hashf(i.x + 4096.0, i.y + 4096.0);
    let b = hashf(i.x + 4097.0, i.y + 4096.0);
    let c = hashf(i.x + 4096.0, i.y + 4097.0);
    let d = hashf(i.x + 4097.0, i.y + 4097.0);
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 4; i++) {
        sum += amp * value_noise(q);
        q = q * 2.03 + vec2(17.0, 9.0);
        amp *= 0.5;
    }
    return sum;
}

fn ease_in(t: f32) -> f32 { let c = clamp(t, 0.0, 1.0); return c * c * c; }
fn ease_out(t: f32) -> f32 { return 1.0 - pow(1.0 - clamp(t, 0.0, 1.0), 3.0); }
fn ease_in_out(t: f32) -> f32 {
    let c = clamp(t, 0.0, 1.0);
    return select(1.0 - pow(-2.0 * c + 2.0, 3.0) / 2.0, 4.0 * c * c * c, c < 0.5);
}
fn ease_out_back(t: f32) -> f32 { let u = t - 1.0; return 1.0 + 2.70158 * u * u * u + 1.70158 * u * u; }
fn ease_out_bounce(t: f32) -> f32 {
    let n = 7.5625;
    let d = 2.75;
    if t < 1.0 / d { return n * t * t; }
    if t < 2.0 / d { let u = t - 1.5 / d; return n * u * u + 0.75; }
    if t < 2.5 / d { let u = t - 2.25 / d; return n * u * u + 0.9375; }
    let u = t - 2.625 / d;
    return n * u * u + 0.984375;
}

// ---------------------------------------------------------------- images

fn img_index(x: u32, y: u32) -> u32 { return y * img_w() + x; }

fn img_load(base: u32, p: vec2<f32>) -> vec4<f32> {
    let x = u32(clamp(p.x, 0.0, width() - 1.0));
    let y = u32(clamp(p.y, 0.0, height() - 1.0));
    return state[params[base] + img_index(x, y)];
}

fn grid_index(x: u32, y: u32) -> u32 { return y * grid_w() + x; }

fn grid_load(base: u32, x: i32, y: i32) -> vec4<f32> {
    let gx = u32(clamp(x, 0, i32(grid_w()) - 1));
    let gy = u32(clamp(y, 0, i32(grid_h()) - 1));
    return state[params[base] + grid_index(gx, gy)];
}

/// Bilinear sample of a grid buffer at sample position `p`.
fn grid_sample(base: u32, p: vec2<f32>) -> vec4<f32> {
    let g = p / 2.0 - 0.5;
    let i = vec2<i32>(floor(g));
    let f = g - floor(g);
    let a = grid_load(base, i.x, i.y);
    let b = grid_load(base, i.x + 1, i.y);
    let c = grid_load(base, i.x, i.y + 1);
    let d = grid_load(base, i.x + 1, i.y + 1);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

/// Sample `p` of this frame's final image (`which` 0) or the last one's (1).
fn final_index(which: u32, p: vec2<f32>) -> u32 {
    let x = u32(clamp(p.x, 0.0, width() - 1.0));
    let y = u32(clamp(p.y, 0.0, height() - 1.0));
    let slot = (mark(MARK_FRAME) + which) & 1u;
    return slot * img_w() * img_h() + img_index(x, y);
}

fn final_load(which: u32, p: vec2<f32>) -> vec3<f32> { return unpack4x8unorm(frames[final_index(which, p)]).rgb; }

fn bloom_load(base: u32, level: u32, x: i32, y: i32) -> vec4<f32> {
    let bx = u32(clamp(x, 0, i32(bloom_w(level)) - 1));
    let by = u32(clamp(y, 0, i32(bloom_h(level)) - 1));
    return state[params[base] + by * bloom_w(level) + bx];
}
