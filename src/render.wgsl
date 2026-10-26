// The whole frame, drawn per terminal cell. See gpu.rs for the buffers and
// pack.rs for the layout of `params`; the constants below must match it.

@group(0) @binding(0) var<storage, read> params: array<u32>;
@group(0) @binding(1) var<storage, read> art: array<u32>;
@group(0) @binding(2) var<storage, read> points: array<u32>;
@group(0) @binding(3) var<storage, read> glyphs: array<u32>;
@group(0) @binding(4) var<storage, read_write> output: array<u32>;
@group(0) @binding(5) var<storage, read_write> cover: array<atomic<u32>>;
@group(0) @binding(6) var<storage, read_write> color: array<atomic<u32>>;

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
const H_FROM: u32 = 16u;
const H_TO: u32 = 80u;

// A scene, from its base.
const S_SHADER: u32 = 0u;
const S_BACKDROP: u32 = 1u;
const S_IDLE: u32 = 2u;
const S_AMPLITUDE: u32 = 3u;
const S_PIECES: u32 = 4u;
const S_STOPS: u32 = 5u;
const S_BACKGROUND: u32 = 8u;
const S_ACCENT: u32 = 11u;
const S_STOP0: u32 = 14u;
const S_PIECE0: u32 = 32u;
const PIECE_LEN: u32 = 8u;

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

const COLS: u32 = 4u;
const ROWS: u32 = 8u;
const SAMPLES: u32 = 32u;
const PI: f32 = 3.14159265;
const TAU: f32 = 6.28318531;
const WHITE: vec3<f32> = vec3<f32>(1.0);

fn pf(i: u32) -> f32 { return bitcast<f32>(params[i]); }
fn time() -> f32 { return pf(H_TIME); }
fn progress() -> f32 { return pf(H_PROGRESS); }
fn kind() -> u32 { return params[H_KIND]; }
fn width() -> f32 { return f32(params[H_COLS] * COLS); }
fn height() -> f32 { return f32(params[H_ROWS] * ROWS); }

fn hash(x: u32, y: u32) -> f32 {
    var h = (x * 0x9e3779b1u) ^ (y * 0x85ebca77u);
    h ^= h >> 15u;
    h *= 0x2c1b3c6du;
    h ^= h >> 12u;
    return f32(h & 0xffffffu) / 16777216.0;
}

fn hashf(x: f32, y: f32) -> f32 { return hash(u32(max(x, 0.0)), u32(max(y, 0.0))); }

fn smoothed(e0: f32, e1: f32, x: f32) -> f32 { return smoothstep(e0, e1, x); }
fn wrap(x: f32) -> f32 { return x - floor(x); }

/// Blends in linear light.
fn mixl(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> {
    return sqrt(max(mix(a * a, b * b, clamp(t, 0.0, 1.0)), vec3(0.0)));
}

fn lerp(a: vec3<f32>, b: vec3<f32>, t: f32) -> vec3<f32> { return mix(a, b, clamp(t, 0.0, 1.0)); }

fn hsl(h: f32, s: f32, l: f32) -> vec3<f32> {
    let a = s * min(l, 1.0 - l);
    let k = (vec3(0.0, 8.0, 4.0) + wrap(h) * 12.0) % vec3(12.0);
    return l - a * clamp(min(k - 3.0, 9.0 - k), vec3(-1.0), vec3(1.0));
}

fn hex(h: u32) -> vec3<f32> {
    return vec3(f32((h >> 16u) & 0xffu), f32((h >> 8u) & 0xffu), f32(h & 0xffu)) / 255.0;
}

// ---------------------------------------------------------------- themes

fn rgb3(i: u32) -> vec3<f32> { return vec3(pf(i), pf(i + 1u), pf(i + 2u)); }
fn background(s: u32) -> vec3<f32> { return rgb3(s + S_BACKGROUND); }
fn accent(s: u32) -> vec3<f32> { return rgb3(s + S_ACCENT); }

fn sample(s: u32, t: f32) -> vec3<f32> {
    let n = max(params[s + S_STOPS], 1u);
    let p = wrap(t) * f32(n);
    let i = u32(p) % n;
    let a = rgb3(s + S_STOP0 + i * 3u);
    let b = rgb3(s + S_STOP0 + ((i + 1u) % n) * 3u);
    return mix(a, b, smoothstep(0.0, 1.0, fract(p)));
}

fn sweep(position: f32, t: f32) -> f32 {
    let phase = (t * 0.35 - floor(t * 0.35 / 2.0) * 2.0) * 1.6 - 0.3;
    return exp(-pow(position * 0.8 - phase, 2.0) * 60.0);
}

fn sparkle(u: f32, v: f32, t: f32) -> f32 {
    let cell = (u32(max(u * 40.0, 0.0)) * 7919u) ^ (u32(max(v * 20.0, 0.0)) * 104729u);
    let phase = f32(cell % 997u) / 997.0;
    let s = sin((t * 0.8 + phase * 13.0) * TAU);
    return select(0.0, (s - 0.985) / 0.015, s > 0.985);
}

/// The scene's shader: ink colour at `(u, v)` within the art.
fn shade_ink(s: u32, u: f32, v: f32, t: f32, row: u32) -> vec3<f32> {
    let d = length(vec2(u, v) - 0.5);
    switch params[s + S_SHADER] {
        case 0u: { return sample(s, 0.6 * u + 0.4 * v - 0.08 * t); }
        case 1u: {
            let w = sin(u * 7.0 + t * 0.9) + sin(v * 9.0 - t * 1.1) + sin((u + v) * 5.0 + t * 0.6)
                + sin(d * 12.0 - t * 1.4);
            return sample(s, w * 0.125 + 0.5 + t * 0.02);
        }
        case 2u: { return sample(s, d * 1.6 - t * 0.15); }
        case 3u: { return lerp(sample(s, u * 0.5 + t * 0.03), WHITE, sweep(u + v * 0.6, t) * 0.8); }
        case 4u: { return hsl(u * 0.9 + v * 0.3 - t * 0.1, 0.85, 0.62); }
        case 5u: {
            let line = select(0.72, 1.0, row % 2u == 0u);
            let roll_at = t * 0.25 - floor(t * 0.25 / 1.4) * 1.4;
            let roll = exp(-pow((v - roll_at + 0.2) * 9.0, 2.0));
            let flicker = 0.97 + 0.03 * sin(t * 13.0 * TAU);
            return lerp(sample(s, v * 0.8 + t * 0.05) * line * flicker, WHITE, roll * 0.35);
        }
        case 6u: {
            let horizon = 0.55 + 0.03 * sin(u * 6.0 + t * 0.7);
            var chrome: vec3<f32>;
            if v < horizon {
                chrome = mixl(hex(0x1a2a6cu), lerp(sample(s, 0.55), WHITE, 0.55), v / horizon);
            } else {
                let ground = lerp(sample(s, 0.1), hex(0xffcf70u), 0.4);
                chrome = mixl(hex(0x2b1200u), ground, pow((v - horizon) / (1.0 - horizon), 0.7));
            }
            return lerp(chrome, WHITE, sweep(u + v * 0.3, t) * 0.9);
        }
        case 7u: {
            let angle = atan2(v - 0.5, u - 0.5) / TAU;
            let foil = hsl(angle * 2.0 + d * 1.5 - t * 0.12, 0.7, 0.72);
            return lerp(mixl(foil, sample(s, u + t * 0.05), 0.35), WHITE, sparkle(u, v, t) * 0.9);
        }
        case 8u: {
            let flicker = sin(u * 23.0 + t * 7.0) * 0.5 + sin(u * 41.0 - t * 11.0) * 0.25;
            let heat = clamp(1.15 - v + flicker * 0.12 + sin(t * 3.0 + u * 9.0) * 0.05, 0.0, 1.0);
            let flame = mixl(mixl(mixl(hex(0x400000u), hex(0xff3000u), heat * 1.6), hex(0xffd23fu), (heat - 0.5) * 2.0),
                WHITE, (heat - 0.85) * 4.0);
            return mixl(flame, sample(s, heat * 0.5), 0.25);
        }
        default: {
            let w = wrap((u * 2.0 + v * 1.2) * 4.0 - t * 0.6);
            let band = floor(w * 2.0) / 2.0;
            return lerp(sample(s, band * 0.5 + t * 0.03), WHITE, select(0.0, 0.4, w < 0.06));
        }
    }
}

// ---------------------------------------------------------------- backdrops

/// Background at `(x, y)` in `0..1`, half-cell pixel `(px, py)`.
fn backdrop(s: u32, x: f32, y: f32, px: u32, py: u32, t: f32) -> vec3<f32> {
    let aspect = pf(H_ASPECT);
    let dx = x - 0.5;
    let dy = y - 0.5;
    let dither = hash(px, py + 7u) * 2.5 / 255.0;
    let base = background(s) * (1.6 - 1.8 * (dx * dx + dy * dy)) + vec3(dither);
    switch params[s + S_BACKDROP] {
        case 1u: {
            let seed = hash(px / 2u, py / 4u);
            if seed > 0.006 { return base; }
            let twinkle = sin(((t * (0.5 + seed * 300.0)) + seed * 1000.0) * TAU) * 0.5 + 0.5;
            return base + lerp(WHITE, sample(s, seed * 100.0), 0.4) * pow(twinkle, 3.0) * 0.75;
        }
        case 2u: {
            let horizon = 0.58;
            let glow = sample(s, 0.15) * 0.35 * exp(-pow((y - horizon) * 14.0, 2.0));
            if y < horizon {
                return base + sample(s, 0.75) * 0.12 * pow(y / horizon, 2.0) + glow;
            }
            let depth = 1.0 / (y - horizon + 0.02);
            let z = depth * 0.6 + t * 1.2;
            let lane = dx * aspect * depth * 0.9;
            let fade = pow((y - horizon) / (1.0 - horizon), 0.6);
            let grid = max(line(z, 0.12), line(lane, 0.08 * min(depth, 10.0) / 4.0));
            return base + sample(s, 0.0) * grid * 0.45 * fade + glow;
        }
        case 3u: {
            let wave = sin(x * 3.0 + t * 0.15) * 0.08 + sin(x * 7.0 - t * 0.1) * 0.04;
            let band = exp(-pow((y - 0.32 - wave) * 6.0, 2.0));
            let rays = sin(x * 40.0 + sin(x * 5.0 + t * 0.3) * 6.0) * 0.35 + 0.65;
            return base + sample(s, x * 0.6 + t * 0.02) * band * rays * 0.28;
        }
        case 4u: {
            let angle = atan2(dy / max(aspect, 0.1), dx) / TAU;
            let beam = abs(wrap(angle * 14.0 + t * 0.04) - 0.5) * 2.0;
            let falloff = max(1.0 - sqrt(dx * dx + dy * dy) * 1.4, 0.0);
            return base + sample(s, 0.3) * beam * beam * falloff * 0.12;
        }
        default: { return base; }
    }
}

fn line(v: f32, w: f32) -> f32 { return max(1.0 - abs(wrap(v) - 0.5) * 2.0 / w, 0.0); }

// ---------------------------------------------------------------- art

struct Piece { x: f32, y: f32, w: f32, h: f32, radius: f32, offset: u32, stride: u32, rows: u32 }

fn piece(s: u32, i: u32) -> Piece {
    let b = s + S_PIECE0 + i * PIECE_LEN;
    return Piece(pf(b), pf(b + 1u), f32(params[b + 2u]), f32(params[b + 3u]), f32(params[b + 4u]),
        params[b + 5u], params[b + 6u], params[b + 7u]);
}

/// Where screen position `(x, y)` lands on the piece at time `t`.
fn local(s: u32, p: Piece, x: f32, y: f32, t: f32) -> vec2<f32> {
    var l = vec2(x - p.x, y - p.y);
    let c = vec2(p.w, p.h) / 2.0;
    let amplitude = pf(s + S_AMPLITUDE);
    switch params[s + S_IDLE] {
        case 1u: { l.y -= amplitude * sin(t * 0.9); }
        case 2u: { l.y -= amplitude * sin(l.x / max(p.w, 1.0) * TAU * 0.9 - t * 2.2); }
        case 3u: { l = c + (l - c) / (1.0 + 0.035 * sin(t * 1.2)); }
        case 4u: {
            let a = 0.04 * sin(t * 0.7);
            let d = l - c;
            l = c + vec2(d.x * cos(a) + d.y * sin(a), -d.x * sin(a) + d.y * cos(a));
        }
        default: {}
    }
    return l;
}

/// The packed pixel (ink, material, its parameter, glow) under `l`.
fn texel(p: Piece, l: vec2<f32>) -> u32 {
    let g = l + p.radius;
    if g.x < 0.0 || g.y < 0.0 { return 0u; }
    let gx = u32(g.x);
    let gy = u32(g.y);
    if gx >= p.stride || gy >= p.rows { return 0u; }
    return art[p.offset + gy * p.stride + gx];
}

fn inside(p: Piece, l: vec2<f32>) -> bool { return l.x >= 0.0 && l.y >= 0.0 && l.x < p.w && l.y < p.h; }

fn coverage(s: u32, x: f32, y: f32, t: f32) -> f32 {
    var best = 0.0;
    for (var i = 0u; i < params[s + S_PIECES]; i++) {
        let p = piece(s, i);
        let l = local(s, p, x, y, t);
        if inside(p, l) { best = max(best, f32(texel(p, l) & 0xffu) / 255.0); }
    }
    return best;
}

/// Colour (rgb) and glow (a) of the art around `(x, y)`.
fn shade(s: u32, x: f32, y: f32, t: f32, row: u32) -> vec4<f32> {
    var best = vec4(0.0);
    var weight = -1.0;
    for (var i = 0u; i < params[s + S_PIECES]; i++) {
        let p = piece(s, i);
        let l = local(s, p, x, y, t);
        let px = texel(p, l);
        let glow = f32(px >> 24u) / 255.0;
        let ink = select(0.0, f32(px & 0xffu) / 255.0, inside(p, l));
        if ink + glow <= weight { continue; }
        weight = ink + glow;
        let uv = l / vec2(max(p.w, 1.0), max(p.h, 1.0));
        let c = shade_ink(s, uv.x, min(uv.y, 1.0), t, row);
        let param = f32((px >> 16u) & 0xffu) / 255.0;
        var surface = c;
        switch (px >> 8u) & 0xffu {
            case 1u: { surface = lerp(c, WHITE, 0.75); }
            case 2u: { surface = select(background(s) * 2.5, accent(s), t - floor(t / 5.0) * 5.0 < 0.15); }
            case 3u: { surface = lerp(background(s), c, 0.1); }
            case 4u: { surface = c * 0.55; }
            case 5u: { surface = select(lerp(background(s), c, 0.1), accent(s), fract(t * 1.2) < 0.5); }
            case 6u: { surface = mixl(c, background(s), 0.45 + 0.35 * param); }
            case 7u: { surface = mixl(background(s), c, param); }
            default: {}
        }
        best = vec4(surface, glow);
    }
    return best;
}

// ---------------------------------------------------------------- transitions

/// Scene `layer` (0 the old scene, 1 the new) seen at screen `(x, y)`:
/// where to look it up (xy) and whether to draw it there at all (z > 0).
fn warp(layer: u32, x: f32, y: f32) -> vec3<f32> {
    let p = progress();
    let w = width();
    let h = height();
    let c = vec2(w, h) / 2.0;
    switch kind() {
        case 9u: {
            var s = 0.15 + 0.85 * ease_out_back(clamp((p - 0.35) / 0.65, 0.0, 1.0));
            if layer == 0u { s = 1.0 + 3.0 * ease_in(p / 0.6); }
            return vec3(c + (vec2(x, y) - c) / s, 1.0);
        }
        case 10u, 11u: {
            let strength = sin(p * PI);
            let d = vec2(x, y) - c;
            let r = length(d);
            if kind() == K_SWIRL {
                let a = strength * TAU * 1.25 * pow(max(1.0 - r / (max(w, h) * 0.6), 0.0), 2.0);
                return vec3(c + vec2(d.x * cos(a) - d.y * sin(a), d.x * sin(a) + d.y * cos(a)), 1.0);
            }
            let wave = strength * w * 0.035 * sin(r / (w * 0.08) * TAU - p * 14.0);
            let scale = select(1.0, (r + wave) / r, r > 0.0);
            return vec3(c + d * scale, 1.0);
        }
        case 12u: {
            let block = 1.0 + pow(sin(p * PI), 1.2) * min(w, h) * 0.09;
            return vec3((floor(vec2(x, y) / block) + 0.5) * block, 1.0);
        }
        case 13u: {
            let band = clamp(floor(y / h * 10.0), 0.0, 9.0);
            let dir = select(-1.0, 1.0, u32(band) % 2u == 0u);
            let delay = band / 10.0 * 0.25;
            if layer == 0u {
                return vec3(x - dir * ease_in(clamp((p - delay) / 0.45, 0.0, 1.0)) * w * 1.1, y, 1.0);
            }
            return vec3(x + dir * (1.0 - ease_out(clamp((p - 0.3 - delay) / 0.45, 0.0, 1.0))) * w * 1.1, y, 1.0);
        }
        case 14u: { return vec3(x - glitch_shift(y), y, 1.0); }
        case 15u: {
            let sq = crt_squash();
            if abs(x - c.x) >= w / 2.0 * sq.x || abs(y - c.y) >= h / 2.0 * sq.y { return vec3(0.0); }
            return vec3(c + (vec2(x, y) - c) / sq, 1.0);
        }
        default: { return vec3(x, y, 1.0); }
    }
}

/// How much of scene `layer` shows at screen `(x, y)`.
fn opacity(layer: u32, x: f32, y: f32) -> f32 {
    let p = progress();
    let k = kind();
    var to = 1.0;
    switch k {
        case 0u: { to = 1.0; }
        case 1u, 2u, 3u, 4u, 5u, 6u, 7u, 8u: { to = wipe_mix(x / width(), y / height()); }
        case 9u: { if layer == 0u { return 1.0 - smoothed(0.3, 0.6, p); } to = smoothed(0.35, 0.55, p); }
        case 10u, 11u: { to = smoothed(0.4, 0.6, p); }
        case 12u: { to = smoothed(0.45, 0.55, p); }
        case 13u: { return 1.0; }
        case 14u: { to = select(0.0, 1.0, glitch_shows_new(y)); }
        case 15u: { to = select(0.0, 1.0, p >= 0.5); }
        default: {
            if layer == 0u { return 1.0 - smoothed(0.0, 0.06, p); }
            return smoothed(0.94, 1.0, p);
        }
    }
    return select(1.0 - to, to, layer == 1u);
}

/// The new scene's share of the background at `(x, y)` in `0..1`.
fn backdrop_mix(x: f32, y: f32) -> f32 {
    let p = progress();
    switch kind() {
        case 0u: { return 1.0; }
        case 1u, 2u, 3u, 4u, 5u, 6u, 7u, 8u: { return wipe_mix(x, y); }
        case 9u: { return smoothed(0.35, 0.55, p); }
        case 10u, 11u: { return smoothed(0.4, 0.6, p); }
        case 12u: { return smoothed(0.45, 0.55, p); }
        case 13u: {
            let band = clamp(floor(y * 10.0), 0.0, 9.0);
            return ease_out(clamp((p - 0.3 - band / 10.0 * 0.25) / 0.45, 0.0, 1.0));
        }
        case 14u: { return select(0.0, 1.0, glitch_shows_new(y * height())); }
        case 15u: { return select(0.0, 1.0, p >= 0.5); }
        default: { return smoothed(0.2, 0.8, p); }
    }
}

/// Extra light at screen `(x, y)`.
fn light(x: f32, y: f32) -> vec3<f32> {
    let p = progress();
    let w = width();
    let h = height();
    let c = vec2(w, h) / 2.0;
    let k = kind();
    if k >= 1u && k <= K_BURN {
        let m = wipe_mix(x / w, y / h);
        let band = 4.0 * m * (1.0 - m);
        let col = select(accent(H_TO), lerp(hex(0xff5a00u), hex(0xffe08au), band), k == K_BURN);
        return col * band * band * 0.7;
    }
    if k == K_ZOOM {
        let flash = exp(-pow((p - 0.45) * 9.0, 2.0)) * 0.3;
        let dist = min(length((vec2(x, y) - c) / vec2(w, h)), 1.0);
        return accent(H_TO) * flash * (1.0 - dist);
    }
    if k == K_GLITCH {
        let step = u32(time() * 24.0);
        let intensity = pow(sin(p * PI), 0.7);
        let band = glitch_band(y);
        let noise = hash(u32(x / 2.0) + step * 7919u, u32(y / 2.0));
        let lit = select(0.0, 0.25, hash(band, step + 31u) < intensity * 0.08) + select(0.0, 0.5, noise < intensity * 0.05);
        var tint = WHITE;
        if glitch_torn(y) {
            let pick = u32(hash(band, step + 21u) * 3.0);
            if pick == 0u { tint = vec3(1.0, 0.25, 0.35); } else if pick == 1u { tint = vec3(0.25, 1.0, 0.9); }
        }
        return tint * lit;
    }
    if k == K_CRT {
        let sq = crt_squash();
        let q = crt_q();
        let thickness = max(sq.y * h / 2.0, 1.5);
        let across = max(1.0 - pow(abs(x - c.x) / (w / 2.0 * sq.x + 2.0), 4.0), 0.0);
        return WHITE * exp(-pow((y - c.y) / thickness, 2.0)) * across * q * 0.9;
    }
    return vec3(0.0);
}

fn wipe_mix(x: f32, y: f32) -> f32 {
    let edge = select(0.1, 0.16, kind() == K_BURN);
    return clamp((progress() * (1.0 + edge) - field(x, y)) / edge, 0.0, 1.0);
}

/// When each place gets wiped, for `(x, y)` in `0..1`.
fn field(x: f32, y: f32) -> f32 {
    let aspect = width() / height();
    let d = vec2((x - 0.5) * aspect, y - 0.5);
    let radius = length(d) / sqrt(0.25 * aspect * aspect + 0.25);
    let angle = wrap(atan2(d.y, d.x) / TAU + 0.25);
    switch kind() {
        case 1u: { return radius; }
        case 2u: { return (abs(d.x) + abs(d.y)) / (0.5 * aspect + 0.5); }
        case 3u: { return angle; }
        case 4u: { return 0.55 * radius + 0.45 * wrap(angle + radius * 1.5); }
        case 5u: { return (x + y) / 2.0; }
        case 6u: { return fract(y * 8.0) * 0.7 + x * 0.3; }
        case 7u: { return hashf(x * 16.0, y * 9.0) * 0.8 + x * 0.2; }
        default: {
            let n = value_noise(x * 5.0 * aspect, y * 5.0) * 0.65 + value_noise(x * 13.0 * aspect, y * 13.0) * 0.35;
            return clamp(n * 1.4 - 0.2, 0.0, 1.0);
        }
    }
}

fn value_noise(x: f32, y: f32) -> f32 {
    let i = floor(vec2(x, y));
    let f = vec2(x, y) - i;
    let s = f * f * (3.0 - 2.0 * f);
    let a = hashf(i.x, i.y);
    let b = hashf(i.x + 1.0, i.y);
    let c = hashf(i.x, i.y + 1.0);
    let d = hashf(i.x + 1.0, i.y + 1.0);
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn glitch_band(y: f32) -> u32 { return u32(max(y / (f32(ROWS) * 1.5), 0.0)); }
fn glitch_step() -> u32 { return u32(time() * 24.0); }
fn glitch_intensity() -> f32 { return pow(sin(progress() * PI), 0.7); }
fn glitch_torn(y: f32) -> bool { return hash(glitch_band(y), glitch_step()) < glitch_intensity() * 0.6; }

fn glitch_shift(y: f32) -> f32 {
    if !glitch_torn(y) { return 0.0; }
    return (hash(glitch_band(y), glitch_step() + 7u) - 0.5) * width() * 0.3 * glitch_intensity();
}

fn glitch_shows_new(y: f32) -> bool {
    return progress() + (hash(glitch_band(y), glitch_step() + 13u) - 0.5) * 0.5 * glitch_intensity() >= 0.5;
}

fn crt_q() -> f32 {
    let p = progress();
    return select(min((1.0 - p) / 0.45, 1.0), min(p / 0.45, 1.0), p < 0.5);
}

/// How far the old television has squashed: (x, y) scale.
fn crt_squash() -> vec2<f32> {
    let q = crt_q();
    let sy = select(0.015, 1.0 - ease_in(q / 0.6) * 0.985, q < 0.6);
    let sx = select(1.0 - ease_in((q - 0.6) / 0.4) * 0.99, 1.0, q < 0.6);
    return vec2(sx, sy);
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

fn is_particles(k: u32) -> bool { return k >= K_MORPH; }

// ---------------------------------------------------------------- particles

struct Point { x: f32, y: f32, seed: f32 }

fn point(i: u32) -> Point {
    return Point(bitcast<f32>(points[i * 3u]), bitcast<f32>(points[i * 3u + 1u]), bitcast<f32>(points[i * 3u + 2u]));
}

fn particle_color(s: u32, p: Point) -> vec3<f32> { return shade(s, p.x, p.y, time(), u32(p.y / 4.0)).rgb; }

/// Draws a square particle of `size` samples centred on `(x, y)`.
fn splat(x: f32, y: f32, size: f32, c: vec3<f32>, alpha: f32) {
    if alpha <= 0.0 { return; }
    let cols = params[H_COLS];
    let w = cols * COLS;
    let h = params[H_ROWS] * ROWS;
    let half = size / 2.0;
    let x0 = u32(max(x - half, 0.0));
    let y0 = u32(max(y - half, 0.0));
    let x1 = min(u32(max(x + half, 0.0)), w);
    let y1 = min(u32(max(y + half, 0.0)), h);
    let a = u32(clamp(alpha, 0.0, 1.0) * 255.0);
    for (var sy = y0; sy < y1; sy++) {
        for (var sx = x0; sx < x1; sx++) {
            let cell = (sy / ROWS) * cols + sx / COLS;
            atomicMax(&cover[cell * SAMPLES + (sy % ROWS) * COLS + sx % COLS], a);
        }
    }
    if x < 0.0 || y < 0.0 || x >= f32(w) || y >= f32(h) { return; }
    let sx = u32(x);
    let sy = u32(y);
    let slot = (((sy / ROWS) * cols + sx / COLS) * 2u + select(0u, 1u, sy % ROWS >= ROWS / 2u)) * 4u;
    let premul = clamp(c, vec3(0.0), vec3(1.0)) * f32(a);
    atomicAdd(&color[slot], u32(premul.r));
    atomicAdd(&color[slot + 1u], u32(premul.g));
    atomicAdd(&color[slot + 2u], u32(premul.b));
    atomicAdd(&color[slot + 3u], a);
}

@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) id: vec3<u32>) {
    let cell = id.x;
    if cell >= params[H_COLS] * params[H_ROWS] { return; }
    for (var i = 0u; i < SAMPLES; i++) { atomicStore(&cover[cell * SAMPLES + i], 0u); }
    for (var i = 0u; i < 8u; i++) { atomicStore(&color[cell * 8u + i], 0u); }
}

@compute @workgroup_size(64)
fn particles(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = kind();
    if !is_particles(k) { return; }
    let i = id.x;
    let ns = params[H_SOURCES];
    let nt = params[H_TARGETS];
    let p = progress();
    let t = time();
    let grain = pf(H_GRAIN);
    let w = width();
    let h = height();
    let c = vec2(w, h) / 2.0;

    if k == K_MORPH || k == K_SWARM {
        let n = max(ns, nt);
        if i >= n || ns == 0u || nt == 0u { return; }
        let a = point(i * ns / n);
        let b = point(ns + i * nt / n);
        let swarm = k == K_SWARM;
        let spread = select(0.25, 0.4, swarm);
        let q = ease_in_out((p - a.seed * spread) / (1.0 - spread));
        var pos: vec2<f32>;
        if swarm {
            let knot = vec2(hash(i, 1u) * w, hash(i, 2u) * h);
            pos = (1.0 - q) * (1.0 - q) * vec2(a.x, a.y) + 2.0 * (1.0 - q) * q * knot + q * q * vec2(b.x, b.y);
        } else {
            let d = vec2(b.x - a.x, b.y - a.y);
            let curve = (b.seed - 0.5) * 0.5 * sin(q * PI);
            pos = vec2(a.x + d.x * q - d.y * curve, a.y + d.y * q + d.x * curve);
        }
        splat(pos.x, pos.y, grain, mixl(particle_color(H_FROM, a), particle_color(H_TO, b), q), 1.0);
        return;
    }

    let source = i < ns;
    if !source && i - ns >= nt { return; }
    let pt = point(i);
    let layer = select(H_TO, H_FROM, source);
    switch k {
        case 18u: {
            if source {
                let out = clamp(p / 0.6, 0.0, 1.0);
                let d = vec2(pt.x, pt.y) - c;
                let dir = d / max(length(d), 1.0);
                let speed = (0.5 + pt.seed) * w * 0.7;
                let jitter = (hash(i, 3u) - 0.5) * 0.8;
                let v = vec2(dir.x + jitter, dir.y - 0.6 + jitter);
                splat(pt.x + v.x * speed * out, pt.y + v.y * speed * out + h * 1.5 * out * out, grain,
                    particle_color(layer, pt), 1.0 - out);
            } else {
                let q = ease_out((p - 0.3 - pt.seed * 0.25) / 0.45);
                let angle = hash(i - ns, 4u) * TAU;
                let s = c + vec2(cos(angle) * w, sin(angle) * h);
                splat(s.x + (pt.x - s.x) * q, s.y + (pt.y - s.y) * q, grain, particle_color(layer, pt), min(q, 1.0));
            }
        }
        case 19u, 21u: {
            let crumble = k == K_CRUMBLE;
            if source {
                let column = hash(u32(pt.x / (grain * 3.0)), 5u);
                let delay = select(pt.seed * 0.3, column * 0.35 + (1.0 - pt.y / h) * 0.1, crumble);
                let fall = clamp((p - delay) / 0.45, 0.0, 1.0);
                let drift = select(0.0, (pt.seed - 0.5) * fall * w * 0.06, crumble);
                splat(pt.x + drift, pt.y + fall * fall * h * 1.3, grain, particle_color(layer, pt), 1.0);
            } else {
                var delay: f32;
                var q: f32;
                if crumble {
                    delay = 0.35 + pt.y / h * 0.25 + pt.seed * 0.1;
                    q = ease_out((p - delay) / 0.4);
                } else {
                    delay = 0.3 + (1.0 - pt.y / h) * 0.4 + pt.seed * 0.05;
                    q = ease_out_bounce(clamp((p - delay) / 0.28, 0.0, 1.0));
                }
                if p < delay { return; }
                let start = select(-grain - h * 0.1, h + grain, crumble);
                splat(pt.x, start + (pt.y - start) * q, grain, particle_color(layer, pt), 1.0);
            }
        }
        case 20u: {
            let d = vec2(pt.x, pt.y) - c;
            let r = length(d);
            let a0 = atan2(d.y, d.x);
            if source {
                let q = ease_in((p - pt.seed * 0.2) / 0.5);
                let a = a0 + q * TAU * 1.5;
                let rr = r * (1.0 - q);
                splat(c.x + cos(a) * rr, c.y + sin(a) * rr, grain, particle_color(layer, pt), 1.0 - q * q);
            } else {
                let q = ease_out((p - 0.45 - pt.seed * 0.15) / 0.45);
                if q <= 0.0 { return; }
                let a = a0 - (1.0 - q) * TAU * 1.5;
                let rr = r * q;
                splat(c.x + cos(a) * rr, c.y + sin(a) * rr, grain, particle_color(layer, pt), sqrt(q));
            }
        }
        default: {}
    }
}

// ---------------------------------------------------------------- the frame

/// The centre of half `half` of cell `(cx, cy)`, in samples.
fn half_centre(cx: u32, cy: u32, half: u32) -> vec2<f32> {
    return vec2(f32(cx * COLS) + f32(COLS) / 2.0, f32(cy * ROWS + half * ROWS / 2u) + f32(ROWS) / 4.0);
}

fn pack(c: vec3<f32>) -> u32 {
    let b = vec3<u32>(clamp(c, vec3(0.0), vec3(1.0)) * 255.0 + 0.5) & vec3(0xfeu);
    return (b.r << 16u) | (b.g << 8u) | b.b;
}

/// Average of colours given as a sum of squares and a count.
fn unsquare(sum: vec3<f32>, n: f32) -> vec3<f32> { return select(vec3(0.0), sqrt(sum / n), n > 0.0); }

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let cols = params[H_COLS];
    let rows = params[H_ROWS];
    let cell = id.x;
    if cell == 0u { output[0] = cols; output[1] = rows; }
    if cell >= cols * rows { return; }
    let cx = cell % cols;
    let cy = cell / cols;
    let t = time();
    let k = kind();

    var alpha: array<f32, 32>;
    var ink_acc = array<vec3<f32>, 2>(vec3(0.0), vec3(0.0));
    var weight = array<f32, 2>(0.0, 0.0);
    var base: array<vec3<f32>, 2>;

    // Backgrounds, mixed between the scenes.
    for (var half = 0u; half < 2u; half++) {
        let py = cy * 2u + half;
        let x = (f32(cx) + 0.5) / f32(cols);
        let y = (f32(py) + 0.5) / f32(rows * 2u);
        let m = backdrop_mix(x, y);
        var b = vec3(0.0);
        if k == K_CRT {
            let layer = select(H_TO, H_FROM, progress() < 0.5);
            let c = half_centre(cx, cy, half);
            if warp(0u, c.x, c.y).z > 0.0 { b = backdrop(layer, x, y, cx, py, t) * (1.0 - crt_q()); }
        } else if m >= 1.0 {
            b = backdrop(H_TO, x, y, cx, py, t);
        } else if m <= 0.0 {
            b = backdrop(H_FROM, x, y, cx, py, t);
        } else {
            b = mixl(backdrop(H_FROM, x, y, cx, py, t), backdrop(H_TO, x, y, cx, py, t), m);
        }
        let c = half_centre(cx, cy, half);
        base[half] = b + light(c.x, c.y);
    }

    // Both scenes, warped and faded as the transition says.
    for (var layer = 0u; layer < 2u; layer++) {
        if k == K_STILL && layer == 0u { continue; }
        let s = select(H_FROM, H_TO, layer == 1u);
        let c0 = half_centre(cx, cy, 0u);
        let c1 = half_centre(cx, cy, 1u);
        let ops = array<f32, 2>(opacity(layer, c0.x, c0.y), opacity(layer, c1.x, c1.y));
        if ops[0] <= 0.0 && ops[1] <= 0.0 { continue; }
        for (var i = 0u; i < SAMPLES; i++) {
            let op = ops[i / (SAMPLES / 2u)];
            if op <= 0.0 { continue; }
            let x = f32(cx * COLS + i % COLS) + 0.5;
            let y = f32(cy * ROWS + i / COLS) + 0.5;
            let wp = warp(layer, x, y);
            if wp.z > 0.0 { alpha[i] = min(alpha[i] + coverage(s, wp.x, wp.y, t) * op, 1.0); }
        }
        for (var half = 0u; half < 2u; half++) {
            let c = half_centre(cx, cy, half);
            let wp = warp(layer, c.x, c.y);
            if wp.z <= 0.0 { continue; }
            let op = ops[half];
            let sh = shade(s, wp.x, wp.y, t, cy * 2u + half);
            var col = sh.rgb;
            if k == K_CRT { col = lerp(col, WHITE, crt_q() * 0.8); }
            let wgt = op * (0.02 + coverage(s, wp.x, wp.y, t));
            ink_acc[half] += col * wgt;
            weight[half] += wgt;
            base[half] += col * sh.a * 0.3 * op;
        }
    }

    // Particles splatted by the particle pass.
    if is_particles(k) {
        for (var i = 0u; i < SAMPLES; i++) {
            alpha[i] = min(alpha[i] + f32(atomicLoad(&cover[cell * SAMPLES + i])) / 255.0, 1.0);
        }
        for (var half = 0u; half < 2u; half++) {
            let slot = (cell * 2u + half) * 4u;
            let wgt = f32(atomicLoad(&color[slot + 3u])) / 255.0;
            if wgt > 0.0 {
                ink_acc[half] += vec3(f32(atomicLoad(&color[slot])), f32(atomicLoad(&color[slot + 1u])),
                    f32(atomicLoad(&color[slot + 2u]))) / 255.0;
                weight[half] += wgt;
            }
        }
    }

    var ink: array<vec3<f32>, 2>;
    for (var half = 0u; half < 2u; half++) {
        ink[half] = select(base[half], ink_acc[half] / max(weight[half], 1e-6), weight[half] > 0.0);
    }
    var means = array<f32, 2>(0.0, 0.0);
    for (var i = 0u; i < SAMPLES; i++) { means[i / 16u] += alpha[i] / 16.0; }
    let mean = (means[0] + means[1]) / 2.0;

    var glyph = 0x2580u;
    var fg = mixl(base[0], ink[0], means[0]);
    var bg = mixl(base[1], ink[1], means[1]);
    switch params[H_LOOK] {
        case 0u: {
            var mask = 0u;
            for (var i = 0u; i < SAMPLES; i++) { if alpha[i] >= 0.5 { mask |= 1u << i; } }
            var shape = 0u;
            var best = 64u;
            for (var g = 0u; g < params[H_GLYPHS]; g++) {
                let m = glyphs[g * 2u];
                let d = countOneBits(m ^ mask);
                if d < best { best = d; shape = m; glyph = glyphs[g * 2u + 1u]; }
            }
            if shape == 0u || shape == 0xffffffffu {
                glyph = 0x2580u;
            } else {
                // Each side of the glyph: the average of the samples it covers.
                var on = vec3(0.0);
                var off = vec3(0.0);
                var n_on = 0.0;
                var n_off = 0.0;
                for (var i = 0u; i < SAMPLES; i++) {
                    let h = i / 16u;
                    let c = mixl(base[h], ink[h], alpha[i]);
                    if ((shape >> i) & 1u) == 1u { on += c * c; n_on += 1.0; } else { off += c * c; n_off += 1.0; }
                }
                fg = unsquare(on, n_on);
                bg = unsquare(off, n_off);
            }
        }
        case 1u: {}
        case 2u: {
            // Braille: each dot stands for a 2x2 block of samples.
            var bits = 0u;
            for (var dot = 0u; dot < 8u; dot++) {
                let dc = dot % 2u;
                let dr = dot / 2u;
                let s0 = dr * 2u * COLS + dc * 2u;
                let covered = alpha[s0] + alpha[s0 + 1u] + alpha[s0 + COLS] + alpha[s0 + COLS + 1u];
                let bit = select(6u + dc, dr + 3u * dc, dr < 3u);
                if covered >= 2.0 { bits |= 1u << bit; }
            }
            glyph = select(0x20u, 0x2800u + bits, bits != 0u);
            let top = f32(countOneBits(bits & 0x1bu));
            fg = lerp(mixl(ink[0], ink[1], 1.0 - top / f32(max(countOneBits(bits), 1u))), WHITE, 0.1);
            bg = mixl(base[0], base[1], 0.5);
        }
        case 3u: {
            let ramp = array<u32, 10>(0x20u, 0x2eu, 0x3au, 0x2du, 0x3du, 0x2bu, 0x2au, 0x23u, 0x25u, 0x40u);
            glyph = ramp[min(u32(round(mean * 9.0)), 9u)];
            fg = mixl(ink[0], ink[1], 0.5);
            bg = mixl(base[0], base[1], 0.5);
        }
        default: {
            bg = mixl(base[0], base[1], 0.5);
            if mean >= 0.35 {
                glyph = 0x25cfu;
                fg = lerp(mixl(ink[0], ink[1], 0.5), WHITE, 0.08);
            } else {
                glyph = 0xb7u;
                fg = bg * 1.8 + vec3(0.04);
            }
        }
    }
    output[2u + cell * 3u] = glyph;
    output[3u + cell * 3u] = pack(fg);
    output[4u + cell * 3u] = pack(bg);
}
