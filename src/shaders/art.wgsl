// The art of a scene: its pieces (bitmaps), how they move while holding,
// their distance fields, and how each style colours them.

struct Piece { x: f32, y: f32, w: f32, h: f32, glow: f32, offset: u32 }

fn piece(s: u32, i: u32) -> Piece {
    let b = s + S_PIECE0 + i * PIECE_LEN;
    return Piece(pf(b), pf(b + 1u), f32(params[b + 2u]), f32(params[b + 3u]), pf(b + 4u), params[b + 5u]);
}

/// Which letter column `x` of the piece belongs to.
fn letter(p: Piece, x: f32) -> f32 { return f32(inputs[params[H_IN_ART] + p.offset + u32(x)] >> 24u); }

/// Where screen position `(x, y)` lands on the piece at time `t`.
fn local(s: u32, p: Piece, x: f32, y: f32, t: f32) -> vec2<f32> {
    var l = vec2(x - p.x, y - p.y);
    let c = vec2(p.w, p.h) / 2.0;
    let amplitude = pf(s + S_AMPLITUDE);
    switch params[s + S_IDLE] {
        case I_FLOAT: { l.y -= amplitude * sin(t * 0.9); }
        case I_WAVE: { l.y -= amplitude * sin(l.x / max(p.w, 1.0) * TAU * 0.9 - t * 2.2); }
        case I_BREATHE: { l = c + (l - c) / (1.0 + 0.035 * sin(t * 1.2)); }
        case I_SWAY: { l = c + rot(-0.04 * sin(t * 0.7)) * (l - c); }
        case I_BOUNCE: {
            // Letters hop one after another: each column knows its letter.
            if l.x >= 0.0 && l.x < p.w {
                l.y += amplitude * 3.0 * abs(sin(t * 2.6 - letter(p, l.x) * 0.55));
            }
        }
        case I_ARRIVE: {
            if l.x >= 0.0 && l.x < p.w { l.y += arrival(s, letter(p, l.x)); }
        }
        case I_MARQUEE: { l.x = wrap((l.x + marquee(s)) / ticker(p)) * ticker(p); }
        default: {}
    }
    return l;
}

/// How far above its place a letter still is as it drops in: one after
/// another, bouncing as they land.
fn arrival(s: u32, letter: f32) -> f32 {
    let q = clamp((age(s) - 0.3 - letter * 0.14) / 0.9, 0.0, 1.0);
    return height() * (1.0 - ease_out_bounce(q));
}

/// How far the ticker has scrolled, and how long it is before it repeats.
fn marquee(s: u32) -> f32 { return age(s) * width() * 0.09; }
fn ticker(p: Piece) -> f32 { return max(width(), p.w) + width() * 0.2; }

/// Where the art's point `(x, y)` at rest is now: `local` the other way, so
/// particles leave from and land on the art as it moves.
fn placed(s: u32, x: f32, y: f32) -> vec2<f32> {
    let t = time();
    let amplitude = pf(s + S_AMPLITUDE);
    for (var i = 0u; i < params[s + S_PIECES]; i++) {
        let p = piece(s, i);
        var l = vec2(x - p.x, y - p.y);
        if l.x < 0.0 || l.y < 0.0 || l.x >= p.w || l.y >= p.h { continue; }
        let c = vec2(p.w, p.h) / 2.0;
        switch params[s + S_IDLE] {
            case I_FLOAT: { l.y += amplitude * sin(t * 0.9); }
            case I_WAVE: { l.y += amplitude * sin(l.x / max(p.w, 1.0) * TAU * 0.9 - t * 2.2); }
            case I_BREATHE: { l = c + (l - c) * (1.0 + 0.035 * sin(t * 1.2)); }
            case I_SWAY: { l = c + rot(0.04 * sin(t * 0.7)) * (l - c); }
            case I_BOUNCE: { l.y -= amplitude * 3.0 * abs(sin(t * 2.6 - letter(p, l.x) * 0.55)); }
            case I_ARRIVE: { l.y -= arrival(s, letter(p, l.x)); }
            case I_MARQUEE: {
                let x = wrap((p.x + l.x - marquee(s)) / ticker(p)) * ticker(p);
                return vec2(x, p.y + l.y);
            }
            default: {}
        }
        return vec2(p.x, p.y) + l;
    }
    return vec2(x, y);
}

/// The packed pixel (ink, material, parameter, letter) under `l`.
fn texel(p: Piece, l: vec2<f32>) -> u32 {
    if l.x < 0.0 || l.y < 0.0 || l.x >= p.w || l.y >= p.h { return 0u; }
    return inputs[params[H_IN_ART] + p.offset + u32(l.y) * u32(p.w) + u32(l.x)];
}

fn coverage(s: u32, x: f32, y: f32, t: f32) -> f32 {
    var best = 0.0;
    for (var i = 0u; i < params[s + S_PIECES]; i++) {
        let p = piece(s, i);
        best = max(best, f32(texel(p, local(s, p, x, y, t)) & 0xffu) / 255.0);
    }
    return best;
}

/// Coverage of scene `layer` (0 old, 1 new) for the distance field.
fn layer_coverage(layer: u32, x: f32, y: f32) -> f32 {
    return coverage(select(H_FROM, H_TO, layer == 1u), x, y, time());
}

/// Signed distance in samples to the edge of scene `layer`'s art: negative
/// inside. Jump flooding (jfa.wgsl) builds it on the grid; a bilinear read
/// keeps it smooth enough for normals.
fn sdf(layer: u32, x: f32, y: f32) -> f32 {
    let d = grid_sample(H_FIELD1, vec2(x, y));
    return select(d.x, d.y, layer == 1u);
}

/// What a scene's art adds at a point: its colour and opacity, light around
/// it, and how much it shades what is behind.
struct Art { color: vec3<f32>, alpha: f32, glow: vec3<f32>, shadow: f32, blur: f32 }

fn material(s: u32, px: u32, c: vec3<f32>, t: f32) -> vec3<f32> {
    let param = f32((px >> 16u) & 0xffu) / 255.0;
    switch (px >> 8u) & 0xffu {
        case 1u: { return lerp(c, WHITE, 0.75); }
        case 2u: { return select(background(s) * 2.5, accent(s), t - floor(t / 5.0) * 5.0 < 0.15); }
        case 3u: { return lerp(background(s), c, 0.1); }
        case 4u: { return c * 0.55; }
        case 5u: { return select(lerp(background(s), c, 0.1), accent(s), fract(t * 1.2) < 0.5); }
        case 6u: { return mixl(c, background(s), 0.45 + 0.35 * param); }
        case 7u: { return mixl(background(s), c, param); }
        default: { return c; }
    }
}

/// The piece nearest `(x, y)`, its local position, and the pixel there.
struct Hit { p: Piece, l: vec2<f32>, px: u32 }

fn hit(s: u32, x: f32, y: f32, t: f32) -> Hit {
    var best = Hit(piece(s, 0u), vec2(-1.0), 0u);
    var weight = -1.0;
    for (var i = 0u; i < params[s + S_PIECES]; i++) {
        let p = piece(s, i);
        let l = local(s, p, x, y, t);
        let px = texel(p, l);
        // Prefer the piece under the point, else the nearest one.
        let outside = max(max(-l.x, l.x - p.w), max(-l.y, l.y - p.h));
        let w = f32(px & 0xffu) / 255.0 - max(outside, 0.0) / max(p.w + p.h, 1.0);
        if w > weight { weight = w; best = Hit(p, l, px); }
    }
    return best;
}

/// Near the art's edge, the signed distance to it measured on the art's
/// own pixels instead of read from the coarse field `d`, so corners stay
/// sharp; further out, `d`.
fn crisp(h: Hit, d: f32) -> f32 {
    if abs(d) > 3.5 { return d; }
    let inside = (h.px & 0xffu) >= 128u;
    let base = floor(h.l);
    var nearest = 4.0;
    for (var dy = -3; dy <= 3; dy++) {
        for (var dx = -3; dx <= 3; dx++) {
            let c = base + vec2(f32(dx), f32(dy)) + 0.5;
            if ((texel(h.p, c) & 0xffu) >= 128u) != inside {
                // The edge runs halfway between the two pixels.
                nearest = min(nearest, max(distance(h.l, c) - 0.5, 0.0));
            }
        }
    }
    return select(nearest, -nearest, inside);
}

/// The art of scene `s` (as `layer`) drawn flat at `(x, y)`.
fn art(s: u32, layer: u32, x: f32, y: f32, t: f32, row: u32) -> Art {
    let h = hit(s, x, y, t);
    let uv = h.l / vec2(max(h.p.w, 1.0), max(h.p.h, 1.0));
    let ink = shade_ink(s, uv.x, clamp(uv.y, 0.0, 1.0), t, row);
    let field = sdf(layer, x, y);
    let d = select(field, crisp(h, field), style(s) == ST_OUTLINE || style(s) == ST_NEON);
    let radius = max(pf(s + S_GLOW), 1.0);
    let width = max(pf(s + S_WIDTH), 1.0);
    var out = Art(material(s, h.px, ink, t), f32(h.px & 0xffu) / 255.0, vec3(0.0), 0.0, 0.0);
    let glow = exp(-max(d, 0.0) / radius) * select(0.0, 1.0, d > 0.0);
    out.glow = ink * glow * 0.3;
    switch style(s) {
        case ST_OUTLINE: {
            out.color = ink;
            out.alpha = clamp(min(-d, d + width) + 0.5, 0.0, 1.0);
        }
        case ST_NEON: {
            let centre = d + width * 0.5;
            out.alpha = clamp(1.0 - abs(centre) / (width * 0.9), 0.0, 1.0);
            let core = exp(-pow(centre / (width * 0.35), 2.0));
            out.color = lerp(ink, WHITE, core * 0.8);
            out.glow = ink * exp(-max(d, 0.0) / (radius * 2.0)) * select(0.0, 1.0, d > 0.0) * 0.7;
        }
        case ST_BEVEL: {
            let bevel = width * 1.5;
            let e = 1.5;
            let grad = vec2(sdf(layer, x + e, y) - sdf(layer, x - e, y), sdf(layer, x, y + e) - sdf(layer, x, y - e)) / (2.0 * e);
            let slope = select(0.0, 1.6, -d < bevel);
            let n = normalize(vec3(grad * slope, 1.0));
            let l = normalize(vec3(-0.55, -0.7, 0.9));
            let diffuse = max(dot(n, l), 0.0);
            let spec = pow(max(dot(reflect(-l, n), vec3(0.0, 0.0, 1.0)), 0.0), 28.0);
            out.color = out.color * (0.3 + 0.85 * diffuse) + WHITE * spec * 0.7;
        }
        case ST_SHADOW: {
            let offset = vec2(width * 1.2, width * 1.8);
            let behind = sdf(layer, x - offset.x, y - offset.y);
            out.shadow = clamp(0.5 - behind / radius, 0.0, 1.0) * 0.75;
        }
        default: {}
    }
    return out;
}
