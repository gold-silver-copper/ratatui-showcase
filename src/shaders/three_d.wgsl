// The art as a solid: its distance field extruded into 3D, ray-marched,
// turning slowly under a key light above a reflective floor, with soft
// shadows, ambient occlusion and depth of field.

struct Bounds { centre: vec2<f32>, size: vec2<f32> }

fn art_bounds(s: u32) -> Bounds {
    var lo = vec2(1e9);
    var hi = vec2(-1e9);
    for (var i = 0u; i < params[s + S_PIECES]; i++) {
        let p = piece(s, i);
        lo = min(lo, vec2(p.x, p.y));
        hi = max(hi, vec2(p.x + p.w, p.y + p.h));
    }
    return Bounds((lo + hi) / 2.0, max(hi - lo, vec2(1.0)));
}

/// The solid and how it is placed: `centre` on screen, half its size.
struct Solid { layer: u32, centre: vec2<f32>, half: vec3<f32> }

/// Distance to the extruded art.
fn solid_distance(o: Solid, p: vec3<f32>) -> f32 {
    let profile = sdf(o.layer, o.centre.x + p.x, o.centre.y + p.y);
    if profile >= FAR * 0.5 { return 1e4; }
    let bevel = 1.5;
    let w = vec2(profile + bevel, abs(p.z) - o.half.z + bevel);
    return min(max(w.x, w.y), 0.0) + length(max(w, vec2(0.0))) - bevel;
}

/// Where a ray enters and leaves the solid's box; enter >= leave if never.
fn box_span(o: Solid, ro: vec3<f32>, rd: vec3<f32>) -> vec2<f32> {
    let margin = vec3(3.0);
    let inv = 1.0 / select(rd, vec3(1e-6), abs(rd) < vec3(1e-6));
    let a = (-o.half - margin - ro) * inv;
    let b = (o.half + margin - ro) * inv;
    let near = max(max(min(a.x, b.x), min(a.y, b.y)), min(a.z, b.z));
    let far = min(min(max(a.x, b.x), max(a.y, b.y)), max(a.z, b.z));
    return vec2(max(near, 0.0), far);
}

/// Distance along the ray to the solid, or -1.
fn march(o: Solid, ro: vec3<f32>, rd: vec3<f32>, steps: i32) -> f32 {
    let span = box_span(o, ro, rd);
    if span.x >= span.y { return -1.0; }
    var d = span.x;
    for (var i = 0; i < steps; i++) {
        let dist = solid_distance(o, ro + rd * d);
        if dist < 0.4 { return d; }
        d += max(dist * 0.9, 0.4);
        if d > span.y { break; }
    }
    return -1.0;
}

/// How much light reaches `ro` from direction `rd`: soft shadows.
fn shadow_ray(o: Solid, ro: vec3<f32>, rd: vec3<f32>) -> f32 {
    let span = box_span(o, ro, rd);
    if span.x >= span.y { return 1.0; }
    var lit = 1.0;
    var d = max(span.x, 1.0);
    for (var i = 0; i < 32; i++) {
        let dist = solid_distance(o, ro + rd * d);
        lit = min(lit, 10.0 * dist / d);
        if lit < 0.02 { return 0.0; }
        d += clamp(dist, 0.8, 12.0);
        if d > span.y { break; }
    }
    return clamp(lit, 0.0, 1.0);
}

fn solid_normal(o: Solid, p: vec3<f32>) -> vec3<f32> {
    let e = 1.0;
    let k = vec2(1.0, -1.0);
    return normalize(k.xyy * solid_distance(o, p + k.xyy * e) + k.yyx * solid_distance(o, p + k.yyx * e)
        + k.yxy * solid_distance(o, p + k.yxy * e) + k.xxx * solid_distance(o, p + k.xxx * e));
}

/// The object's turn at time `t`: about the vertical axis, then a tilt.
fn turn(t: f32) -> mat3x3<f32> {
    let a = 0.75 * sin(t * 0.55);
    let b = 0.16 * sin(t * 0.37);
    let ry = mat3x3(cos(a), 0.0, -sin(a), 0.0, 1.0, 0.0, sin(a), 0.0, cos(a));
    let rx = mat3x3(1.0, 0.0, 0.0, 0.0, cos(b), sin(b), 0.0, -sin(b), cos(b));
    return rx * ry;
}

/// Toward the key light, up and to the left in front.
const KEY: vec3<f32> = vec3<f32>(-0.45, -0.75, -0.5);

/// Colour of the solid at `p`, seen from `view`; `full` adds lighting,
/// occlusion and highlights, which reflections can do without.
fn solid_color(s: u32, o: Solid, world: mat3x3<f32>, p: vec3<f32>, view: vec3<f32>, t: f32, row: u32, full: bool) -> vec3<f32> {
    let h = hit(s, o.centre.x + p.x, o.centre.y + p.y, t);
    let uv = h.l / vec2(max(h.p.w, 1.0), max(h.p.h, 1.0));
    let ink = material(s, h.px, shade_ink(s, uv.x, clamp(uv.y, 0.0, 1.0), t, row), t);
    if !full { return ink * 0.6; }
    let n_obj = solid_normal(o, p);
    let n = world * n_obj;
    let light = normalize(KEY);
    let diffuse = max(dot(n, light), 0.0);
    let spec = pow(max(dot(reflect(-light, n), view), 0.0), 32.0);
    let rim = pow(1.0 - abs(dot(n, view)), 3.0);
    let face = smoothstep(0.55, 0.85, abs(n_obj.z));
    // Ambient occlusion: how much of the solid crowds in along the normal.
    var occlusion = 0.0;
    var weight = 1.0;
    for (var k = 1; k <= 4; k++) {
        let reach = f32(k) * 2.0;
        occlusion += (reach - solid_distance(o, p + n_obj * reach)) / reach * weight;
        weight *= 0.6;
    }
    let ao = clamp(1.0 - occlusion * 0.35, 0.3, 1.0);
    let side = mixl(ink * 0.45, accent(s) * 0.5, 0.3);
    let albedo = mix(side, ink, face);
    let sky = sample(s, reflect(-view, n).y * 0.5 + 0.5) * 0.25;
    return (albedo * (0.25 + 0.85 * diffuse) + sky) * ao + WHITE * spec * 0.8 + accent(s) * rim * 0.35;
}

fn turntable(s: u32, layer: u32, x: f32, y: f32, t: f32, row: u32) -> Art {
    let bounds = art_bounds(s);
    let o = Solid(layer, bounds.centre, vec3(bounds.size * 0.5, max(bounds.size.x, bounds.size.y) * 0.08));
    let focal = max(width(), height()) * 2.2;
    let world = turn(t);
    let object = transpose(world);
    let ro_w = vec3(0.0, 0.0, -focal);
    let rd_w = normalize(vec3(x - bounds.centre.x, y - bounds.centre.y, focal));
    // Behind the solid, the background is out of focus.
    var out = Art(vec3(0.0), 0.0, vec3(0.0), 0.0, 2.5);

    let d = march(o, object * ro_w, object * rd_w, 72);
    if d > 0.0 {
        let p = object * ro_w + object * rd_w * d;
        out.color = solid_color(s, o, world, p, -rd_w, t, row, true);
        out.alpha = 1.0;
        out.blur = 0.0;
        return out;
    }

    // The floor: a dark mirror just below the art, fading into the distance.
    if rd_w.y <= 0.0 { return out; }
    let floor_y = o.half.y + 6.0;
    let along = (floor_y - ro_w.y) / rd_w.y;
    let p_w = ro_w + rd_w * along;
    let fade = clamp(1.0 - p_w.z / (o.half.x * 3.0), 0.0, 1.0) * clamp(1.0 + p_w.z / (o.half.x * 2.0), 0.0, 1.0);
    if fade <= 0.0 { return out; }
    let mirrored = vec3(rd_w.x, -rd_w.y, rd_w.z);
    let r = march(o, object * p_w, object * mirrored, 48);
    var seen = 0.0;
    if r > 0.0 {
        out.color = solid_color(s, o, world, object * p_w + object * mirrored * r, -mirrored, t, row, false);
        seen = 1.0;
    }
    let lit = shadow_ray(o, object * p_w, object * normalize(KEY));
    let fresnel = 0.35 + 0.4 * pow(1.0 - abs(rd_w.y), 4.0);
    out.alpha = seen * fresnel * fade;
    out.shadow = (1.0 - lit) * 0.7 * fade;
    out.blur = clamp(abs(p_w.z) / (o.half.x * 2.0), 0.0, 1.0) * 2.0;
    return out;
}
