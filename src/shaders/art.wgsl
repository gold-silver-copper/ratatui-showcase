// The art of a scene: its pieces (bitmaps), how they move while holding,
// their distance fields, and how each style colours them.

struct Piece { x: f32, y: f32, w: f32, h: f32, glow: f32, offset: u32, stride: u32, rows: u32 }

fn piece(s: u32, i: u32) -> Piece {
    let b = s + S_PIECE0 + i * PIECE_LEN;
    return Piece(pf(b), pf(b + 1u), f32(params[b + 2u]), f32(params[b + 3u]), pf(b + 4u),
        params[b + 5u], params[b + 6u], params[b + 7u]);
}

/// Where screen position `(x, y)` lands on the piece at time `t`.
fn local(s: u32, p: Piece, x: f32, y: f32, t: f32) -> vec2<f32> {
    var l = vec2(x - p.x, y - p.y);
    let c = vec2(p.w, p.h) / 2.0;
    let amplitude = pf(s + S_AMPLITUDE);
    // The music makes the art throb.
    l = c + (l - c) / (1.0 + bass() * 0.06);
    switch params[s + S_IDLE] {
        case 1u: { l.y -= amplitude * sin(t * 0.9); }
        case 2u: { l.y -= amplitude * sin(l.x / max(p.w, 1.0) * TAU * 0.9 - t * 2.2); }
        case 3u: { l = c + (l - c) / (1.0 + 0.035 * sin(t * 1.2)); }
        case 4u: {
            let a = 0.04 * sin(t * 0.7);
            l = c + rot(-a) * (l - c);
        }
        default: {}
    }
    return l;
}

/// The packed pixel (ink, material, parameter) under `l`.
fn texel(p: Piece, l: vec2<f32>) -> u32 {
    if l.x < 0.0 || l.y < 0.0 || l.x >= p.w || l.y >= p.h { return 0u; }
    return inputs[params[H_IN_ART] + p.offset + u32(l.y) * p.stride + u32(l.x)];
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
/// inside. Built each frame by jump flooding (jfa.wgsl).
fn sdf(layer: u32, x: f32, y: f32) -> f32 {
    let g = vec2<i32>(i32(x / 2.0), i32(y / 2.0));
    var best = FAR;
    for (var dy = 0; dy < 2; dy++) {
        for (var dx = 0; dx < 2; dx++) {
            let cell = grid_load(H_FIELD0, g.x + dx, g.y + dy);
            let seed = select(cell.xy, cell.zw, layer == 1u);
            if seed.x >= 0.0 { best = min(best, distance(vec2(x, y), seed)); }
        }
    }
    if best >= FAR { return FAR; }
    let inside = layer_coverage(layer, x, y) > 0.5;
    return select(best, -best, inside);
}

/// What a scene's art adds at a point: its colour and opacity, light around
/// it, and how much it shades what is behind.
struct Art { color: vec3<f32>, alpha: f32, glow: vec3<f32>, shadow: f32 }

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

/// The art of scene `s` (as `layer`) drawn flat at `(x, y)`.
fn art(s: u32, layer: u32, x: f32, y: f32, t: f32, row: u32) -> Art {
    let h = hit(s, x, y, t);
    let uv = h.l / vec2(max(h.p.w, 1.0), max(h.p.h, 1.0));
    let ink = shade_ink(s, uv.x, clamp(uv.y, 0.0, 1.0), t, row);
    let d = sdf(layer, x, y);
    let radius = max(pf(s + S_GLOW), 1.0);
    let width = max(pf(s + S_WIDTH), 1.0);
    var out = Art(material(s, h.px, ink, t), f32(h.px & 0xffu) / 255.0, vec3(0.0), 0.0);
    let glow = exp(-max(d, 0.0) / radius) * select(0.0, 1.0, d > 0.0);
    out.glow = ink * glow * (0.3 + level() * 0.4);
    switch style(s) {
        case 1u: {
            out.color = ink;
            out.alpha = clamp(min(-d, d + width) + 0.5, 0.0, 1.0);
        }
        case 2u: {
            let centre = d + width * 0.5;
            out.alpha = clamp(1.0 - abs(centre) / (width * 0.9), 0.0, 1.0);
            let core = exp(-pow(centre / (width * 0.35), 2.0));
            out.color = lerp(ink, WHITE, core * 0.8);
            out.glow = ink * exp(-max(d, 0.0) / (radius * 2.0)) * select(0.0, 1.0, d > 0.0) * 0.7;
        }
        case 3u: {
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
        case 4u: {
            let offset = vec2(width * 1.2, width * 1.8);
            let behind = sdf(layer, x - offset.x, y - offset.y);
            out.shadow = clamp(0.5 - behind / radius, 0.0, 1.0) * 0.75;
        }
        default: {}
    }
    return out;
}
