// Shared by every pass: bindings, the layout of `params` (see gpu.rs, which
// must match), and small helpers.
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

// Pipeline constants: jump size for distance fields, and which of two
// buffers a ping-pong pass reads.
override STEP: u32 = 1u;
override FLIP: u32 = 0u;

// Header.
const H_COLS: u32 = 0u;
const H_ROWS: u32 = 1u;
const H_TIME: u32 = 2u;
const H_ASPECT: u32 = 3u;
const H_LOOK: u32 = 4u;
const H_GLYPHS: u32 = 5u;
const H_KIND: u32 = 6u;
const H_PROGRESS: u32 = 7u;
const H_GRAIN: u32 = 8u;
const H_SOURCES: u32 = 9u;
const H_TARGETS: u32 = 10u;
const H_FRAME: u32 = 11u;
const H_DT: u32 = 12u;
const H_POST: u32 = 13u;
const H_DITHER: u32 = 14u;
const H_SWARM: u32 = 15u;
const H_SCENE: u32 = 16u;
const H_FINAL: u32 = 17u;
const H_PREVIOUS: u32 = 18u;
const H_BLOOM0: u32 = 19u;
const H_BLOOM1: u32 = 20u;
const H_FIELD0: u32 = 21u;
const H_FIELD1: u32 = 22u;
const H_SIM0: u32 = 23u;
const H_SIM1: u32 = 24u;
const H_SIM2: u32 = 25u;
const H_SIM3: u32 = 26u;
const H_PARTICLES: u32 = 27u;
const H_BG_OLD: u32 = 28u;
const H_BG_NEW: u32 = 29u;
const H_BLOOM2: u32 = 30u;
const H_BLOOM3: u32 = 31u;
const H_BLOOM4: u32 = 32u;
const H_BLOOM5: u32 = 33u;
const H_IN_GLYPHS: u32 = 34u;
const H_IN_ASCII: u32 = 35u;
const H_ASCII_COUNT: u32 = 36u;
const H_IN_POINTS: u32 = 37u;
const H_IN_HOMES: u32 = 38u;
const H_HOMES: u32 = 39u;
const H_IN_ART: u32 = 40u;
const H_IN_ORBIT: u32 = 41u;
const H_SIM_LATEST: u32 = 42u;
const H_SIM_RESET: u32 = 43u;
const H_ORBIT_LEN: u32 = 44u;
const H_FLAME: u32 = 45u;
const H_FLAME_THREADS: u32 = 46u;
const H_FLAME_STEPS: u32 = 47u;
const H_FROM: u32 = 64u;
const H_TO: u32 = 128u;

// A scene's block, from its base.
const S_SHADER: u32 = 0u;
const S_BACKDROP: u32 = 1u;
const S_IDLE: u32 = 2u;
const S_AMPLITUDE: u32 = 3u;
const S_PIECES: u32 = 4u;
const S_STOPS: u32 = 5u;
const S_MODE: u32 = 6u;
const S_STYLE: u32 = 7u;
const S_BACKGROUND: u32 = 8u;
const S_ACCENT: u32 = 11u;
const S_STOP0: u32 = 14u;
const S_AGE: u32 = 29u;
const S_WIDTH: u32 = 30u;
const S_GLOW: u32 = 31u;
const S_PIECE0: u32 = 32u;
const S_VARIANT: u32 = 48u;
const PIECE_LEN: u32 = 8u;

// Modes: how a scene draws its art.
const M_FLAT: u32 = 0u;
const M_TURNTABLE: u32 = 1u;
const M_FLUID: u32 = 2u;
const M_REACTION: u32 = 3u;
const M_LIFE: u32 = 4u;
const M_FIRE: u32 = 5u;
const M_RIPPLE: u32 = 6u;
const M_SWARM: u32 = 7u;

// Styles.
const ST_SOLID: u32 = 0u;
const ST_OUTLINE: u32 = 1u;
const ST_NEON: u32 = 2u;
const ST_BEVEL: u32 = 3u;
const ST_SHADOW: u32 = 4u;

// Transitions.
const K_STILL: u32 = 0u;
const K_BURN: u32 = 8u;
const K_ZOOM: u32 = 9u;
const K_SWIRL: u32 = 10u;
const K_RIPPLE: u32 = 11u;
const K_MOSAIC: u32 = 12u;
const K_SLICES: u32 = 13u;
const K_GLITCH: u32 = 14u;
const K_CRT: u32 = 15u;
const K_MORPH: u32 = 16u;
const K_SWARM: u32 = 17u;
const K_SCATTER: u32 = 18u;
const K_POUR: u32 = 19u;
const K_VORTEX: u32 = 20u;
const K_CRUMBLE: u32 = 21u;
const K_MELT: u32 = 22u;

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
fn ease_out_back(t: f32) -> f32 { return 1.0 + 2.70158 * pow(t - 1.0, 3.0) + 1.70158 * pow(t - 1.0, 2.0); }
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

fn bloom_load(base: u32, level: u32, x: i32, y: i32) -> vec4<f32> {
    let bx = u32(clamp(x, 0, i32(bloom_w(level)) - 1));
    let by = u32(clamp(y, 0, i32(bloom_h(level)) - 1));
    return state[params[base] + by * bloom_w(level) + bx];
}
