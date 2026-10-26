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
        case K_ZOOM: {
            var s = 0.15 + 0.85 * ease_out_back(clamp((p - 0.35) / 0.65, 0.0, 1.0));
            if layer == 0u { s = 1.0 + 3.0 * ease_in(p / 0.6); }
            return vec3(c + (vec2(x, y) - c) / s, 1.0);
        }
        case K_SWIRL, K_RIPPLE: {
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
        case K_MOSAIC: {
            let block = 1.0 + pow(sin(p * PI), 1.2) * min(w, h) * 0.09;
            return vec3((floor(vec2(x, y) / block) + 0.5) * block, 1.0);
        }
        case K_SLICES: {
            let band = clamp(floor(y / h * 10.0), 0.0, 9.0);
            let dir = select(-1.0, 1.0, u32(band) % 2u == 0u);
            let delay = band / 10.0 * 0.25;
            if layer == 0u {
                return vec3(x - dir * ease_in(clamp((p - delay) / 0.45, 0.0, 1.0)) * w * 1.1, y, 1.0);
            }
            return vec3(x + dir * (1.0 - ease_out(clamp((p - 0.3 - delay) / 0.45, 0.0, 1.0))) * w * 1.1, y, 1.0);
        }
        case K_GLITCH: { return vec3(x - glitch_shift(y), y, 1.0); }
        case K_CRT: {
            let sq = crt_squash();
            if abs(x - c.x) >= w / 2.0 * sq.x || abs(y - c.y) >= h / 2.0 * sq.y { return vec3(0.0); }
            return vec3(c + (vec2(x, y) - c) / sq, 1.0);
        }
        case K_SHATTER: {
            if layer == 0u { return shatter(vec2(x, y)); }
            return vec3(x, y, 1.0);
        }
        case K_PAGE_CURL: {
            if layer == 1u { return vec3(x, y, 1.0); }
            let c = curl(vec2(x, y));
            return vec3(c.xy, select(0.0, 1.0, c.z > 0.0));
        }
        case K_PIXEL_SORT: { return vec3(sorted_x(layer, x, y), y, 1.0); }
        default: { return vec3(x, y, 1.0); }
    }
}

/// How much of scene `layer` shows at screen `(x, y)`.
fn opacity(layer: u32, x: f32, y: f32) -> f32 {
    let p = progress();
    let k = kind();
    var to = 1.0;
    switch k {
        case K_STILL: { to = 1.0; }
        case K_RADIAL_WIPE, K_DIAMOND_WIPE, K_CLOCK_WIPE, K_SPIRAL_WIPE, K_DIAGONAL_WIPE, K_BLINDS, K_CHECKERBOARD, K_BURN, K_INK_BLEED, K_INFERNO: { to = wipe_mix(x / width(), y / height()); }
        case K_SHATTER: { to = 1.0 - shatter(vec2(x, y)).z; }
        case K_PAGE_CURL: { to = select(0.0, 1.0, curl(vec2(x, y)).z == 0.0); }
        case K_PIXEL_SORT: { to = smoothed(0.45, 0.55, p); }
        case K_DISSOLVE: { if layer == 0u { return 0.0; } to = smoothed(0.35, 0.8, p); }
        case K_ZOOM: { if layer == 0u { return 1.0 - smoothed(0.3, 0.6, p); } to = smoothed(0.35, 0.55, p); }
        case K_SWIRL, K_RIPPLE: { to = smoothed(0.4, 0.6, p); }
        case K_MOSAIC: { to = smoothed(0.45, 0.55, p); }
        case K_SLICES: { return 1.0; }
        case K_GLITCH: { to = select(0.0, 1.0, glitch_shows_new(y)); }
        case K_CRT: { to = select(0.0, 1.0, p >= 0.5); }
        case K_MELT: { return 0.0; }
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
        case K_STILL: { return 1.0; }
        case K_RADIAL_WIPE, K_DIAMOND_WIPE, K_CLOCK_WIPE, K_SPIRAL_WIPE, K_DIAGONAL_WIPE, K_BLINDS, K_CHECKERBOARD, K_BURN, K_INK_BLEED, K_INFERNO: { return wipe_mix(x, y); }
        case K_SHATTER: { return 1.0 - shatter(vec2(x * width(), y * height())).z; }
        case K_PAGE_CURL: { return select(0.0, 1.0, curl(vec2(x * width(), y * height())).z == 0.0); }
        case K_PIXEL_SORT: { return smoothed(0.45, 0.55, p); }
        case K_ZOOM: { return smoothed(0.35, 0.55, p); }
        case K_SWIRL, K_RIPPLE: { return smoothed(0.4, 0.6, p); }
        case K_MOSAIC: { return smoothed(0.45, 0.55, p); }
        case K_SLICES: {
            let band = clamp(floor(y * 10.0), 0.0, 9.0);
            return ease_out(clamp((p - 0.3 - band / 10.0 * 0.25) / 0.45, 0.0, 1.0));
        }
        case K_GLITCH: { return select(0.0, 1.0, glitch_shows_new(y * height())); }
        case K_CRT: { return select(0.0, 1.0, p >= 0.5); }
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
    if k >= K_RADIAL_WIPE && k <= K_BURN {
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
    let k = kind();
    let edge = select(select(0.1, 0.16, k == K_BURN || k == K_INFERNO), 0.3, k == K_INK_BLEED);
    return clamp((progress() * (1.0 + edge) - field(x, y)) / edge, 0.0, 1.0);
}

/// When each place gets wiped, for `(x, y)` in `0..1`.
fn field(x: f32, y: f32) -> f32 {
    let aspect = width() / height();
    let d = vec2((x - 0.5) * aspect, y - 0.5);
    let radius = length(d) / sqrt(0.25 * aspect * aspect + 0.25);
    let angle = wrap(atan2(d.y, d.x) / TAU + 0.25);
    switch kind() {
        case K_RADIAL_WIPE: { return radius; }
        case K_DIAMOND_WIPE: { return (abs(d.x) + abs(d.y)) / (0.5 * aspect + 0.5); }
        case K_CLOCK_WIPE: { return angle; }
        case K_SPIRAL_WIPE: { return 0.55 * radius + 0.45 * wrap(angle + radius * 1.5); }
        case K_DIAGONAL_WIPE: { return (x + y) / 2.0; }
        case K_BLINDS: { return fract(y * 8.0) * 0.7 + x * 0.3; }
        case K_CHECKERBOARD: { return hashf(x * 16.0, y * 9.0) * 0.8 + x * 0.2; }
        case K_INK_BLEED: {
            // Ink soaking out from three drops, its edge feathered by the paper.
            let q = vec2(x * aspect, y);
            let drops = min(min(length(q - vec2(0.3 * aspect, 0.35)), length(q - vec2(0.72 * aspect, 0.6))),
                length(q - vec2(0.5 * aspect, 0.85)));
            let fibres = fbm(q * 9.0 + vec2(fbm(q * 3.0 + 7.0) * 3.0, 3.0));
            return clamp(drops * 1.1 + fibres * 0.45 - 0.2, 0.0, 1.0);
        }
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

// ---------------------------------------------------------------- shatter

fn shard_size() -> f32 { return min(width(), height()) / 6.0; }

/// The jittered centre of shard `cell`.
fn shard_site(cell: vec2<f32>) -> vec2<f32> {
    return (cell + 0.2 + 0.6 * vec2(hashf(cell.x + 50.0, cell.y + 50.0), hashf(cell.x + 90.0, cell.y + 70.0))) * shard_size();
}

/// The shard (Voronoi cell) that holds `p` before anything moves.
fn shard_of(p: vec2<f32>) -> vec2<f32> {
    let c = floor(p / shard_size());
    var best = c;
    var nearest = 1e9;
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let cell = c + vec2(f32(i), f32(j));
            let d = distance(p, shard_site(cell));
            if d < nearest { nearest = d; best = cell; }
        }
    }
    return best;
}

/// Where screen `p` shows the old scene once it has broken into shards that
/// tip and fall, each in its own time (xy), and how much of the shard is
/// left there (z: 0 where no shard covers `p`).
fn shatter(p: vec2<f32>) -> vec3<f32> {
    let size = shard_size();
    let c = floor(p / size);
    // Shards fall, so the one over `p` may come from a few rows above.
    for (var dy = 0; dy <= 4; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let cell = c + vec2(f32(dx), f32(-dy));
            let site = shard_site(cell);
            let h = hashf(cell.x + 7.0, cell.y + 300.0);
            let q = clamp((progress() - 0.05 - h * 0.3 - (1.0 - site.y / height()) * 0.2) / 0.55, 0.0, 1.0);
            let drop = vec2((h - 0.5) * size * 1.2 * q, q * q * height() * 1.1);
            let back = rot(-(h - 0.5) * 2.4 * q) * (p - site - drop) + site;
            if all(shard_of(back) == cell) {
                return vec3(back, 1.0 - smoothstep(0.55, 1.0, q));
            }
        }
    }
    return vec3(0.0);
}

// ---------------------------------------------------------------- page curl

/// The page curl at screen `p`: where on the old scene it shows (xy), what
/// covers it (z: 0 the new scene, 1 the back of the turning page, 2 the old
/// page still flat) and how far from the fold, in curl radii (w).
fn curl(p: vec2<f32>) -> vec4<f32> {
    let corner = vec2(width(), height());
    let d = normalize(vec2(width(), height() * 0.7));
    let radius = min(width(), height()) * 0.18;
    let u = dot(corner - p, d);
    let fold = ease_in_out(progress()) * (dot(corner, d) + radius * 2.0);
    if u < fold { return vec4(p, 0.0, (fold - u) / radius); }
    // The peeled part folds back over the page, mirrored about the fold.
    let back = u - fold;
    if back < min(radius, fold) {
        return vec4(p + d * 2.0 * back, 1.0, back / radius);
    }
    return vec4(p, 2.0, 0.0);
}

// ---------------------------------------------------------------- pixel sort

/// Where sample `(x, y)` of `layer` comes from while rows smear: each row
/// splits into runs, and every sample of a run slides toward its start.
fn sorted_x(layer: u32, x: f32, y: f32) -> f32 {
    let row = u32(max(y, 0.0) / 2.0);
    let run = 20.0 + hash(row, 9u) * 120.0;
    let shift = hash(row, 3u) * run;
    let start = floor((x + shift) / run) * run - shift;
    let delay = hash(row, 5u) * 0.25;
    let q = select(1.0 - ease_out(clamp((progress() - 0.45 - delay) / 0.45, 0.0, 1.0)),
        ease_in(clamp((progress() - delay) / 0.45, 0.0, 1.0)), layer == 0u);
    return start + (x - start) * (1.0 - q * 0.97);
}
