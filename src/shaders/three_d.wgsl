// The art as a solid: its distance field extruded into 3D, ray-marched,
// turning slowly under a key light.

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

/// Distance to the extruded art; `centre` is where it stands on screen.
fn solid(layer: u32, p: vec3<f32>, centre: vec2<f32>, depth: f32) -> f32 {
    let profile = sdf(layer, centre.x + p.x, centre.y + p.y);
    if profile >= FAR { return 1e4; }
    let bevel = 1.5;
    let w = vec2(profile + bevel, abs(p.z) - depth * 0.5 + bevel);
    return min(max(w.x, w.y), 0.0) + length(max(w, vec2(0.0))) - bevel;
}

/// The object's turn at time `t`: about the vertical axis, then a tilt.
fn turn(t: f32) -> mat3x3<f32> {
    let a = 0.75 * sin(t * 0.55);
    let b = 0.16 * sin(t * 0.37);
    let ry = mat3x3(cos(a), 0.0, -sin(a), 0.0, 1.0, 0.0, sin(a), 0.0, cos(a));
    let rx = mat3x3(1.0, 0.0, 0.0, 0.0, cos(b), sin(b), 0.0, -sin(b), cos(b));
    return rx * ry;
}

fn turntable(s: u32, layer: u32, x: f32, y: f32, t: f32, row: u32) -> Art {
    let bounds = art_bounds(s);
    let depth = max(bounds.size.x, bounds.size.y) * 0.16;
    let focal = max(width(), height()) * 2.2;
    let world = turn(t);
    let object = transpose(world);
    let ro = object * vec3(0.0, 0.0, -focal);
    let rd = object * normalize(vec3(x - bounds.centre.x, y - bounds.centre.y, focal));
    let radius = length(vec3(bounds.size * 0.5, depth));
    var d = focal - radius;
    var found = false;
    var p = vec3(0.0);
    for (var i = 0; i < 80; i++) {
        p = ro + rd * d;
        let dist = solid(layer, p, bounds.centre, depth);
        if dist < 0.4 { found = true; break; }
        d += max(dist * 0.9, 0.4);
        if d > focal + radius { break; }
    }
    var out = Art(vec3(0.0), 0.0, vec3(0.0), 0.0);
    if !found { return out; }

    let e = 1.0;
    let k = vec2(1.0, -1.0);
    let n_obj = normalize(
        k.xyy * solid(layer, p + k.xyy * e, bounds.centre, depth) + k.yyx * solid(layer, p + k.yyx * e, bounds.centre, depth)
            + k.yxy * solid(layer, p + k.yxy * e, bounds.centre, depth) + k.xxx * solid(layer, p + k.xxx * e, bounds.centre, depth));
    let n = world * n_obj;
    let view = -(world * rd);
    let light = normalize(vec3(-0.5, -0.75, -0.8));
    let diffuse = max(dot(n, light), 0.0);
    let spec = pow(max(dot(reflect(-light, n), view), 0.0), 32.0);
    let rim = pow(1.0 - abs(dot(n, view)), 3.0);
    let face = smoothstep(0.55, 0.85, abs(n_obj.z));

    let h = hit(s, bounds.centre.x + p.x, bounds.centre.y + p.y, t);
    let uv = h.l / vec2(max(h.p.w, 1.0), max(h.p.h, 1.0));
    let ink = material(s, h.px, shade_ink(s, uv.x, clamp(uv.y, 0.0, 1.0), t, row), t);
    let side = mixl(ink * 0.45, accent(s) * 0.5, 0.3);
    let albedo = mix(side, ink, face);
    let env = sample(s, reflect(-view, n).y * 0.5 + 0.5) * 0.25;
    out.color = albedo * (0.25 + 0.85 * diffuse) + WHITE * spec * 0.8 + accent(s) * rim * 0.35 + env;
    out.alpha = 1.0;
    return out;
}
