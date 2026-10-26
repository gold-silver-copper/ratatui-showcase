// Transitions: how each scene is warped and faded, how the backgrounds mix,
// and the light a transition adds. Layer 0 is the old scene, 1 the new.

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
        case 22u: { return 0.0; }
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
        let tick = u32(time() * 24.0);
        let intensity = pow(sin(p * PI), 0.7);
        let band = glitch_band(y);
        let noise = hash(u32(x / 2.0) + tick * 7919u, u32(y / 2.0));
        let lit = select(0.0, 0.25, hash(band, tick + 31u) < intensity * 0.08) + select(0.0, 0.5, noise < intensity * 0.05);
        var tint = WHITE;
        if glitch_torn(y) {
            let pick = u32(hash(band, tick + 21u) * 3.0);
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
            let n = value_noise(vec2(x * 5.0 * aspect, y * 5.0)) * 0.65 + value_noise(vec2(x * 13.0 * aspect, y * 13.0)) * 0.35;
            return clamp(n * 1.4 - 0.2, 0.0, 1.0);
        }
    }
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
