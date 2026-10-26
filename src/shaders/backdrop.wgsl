// Backgrounds: soft gradients, animated fields, 2D fractals and ray-marched
// 3D fractals. `(x, y)` are in 0..1 across the screen.

fn line(v: f32, w: f32) -> f32 { return max(1.0 - abs(wrap(v) - 0.5) * 2.0 / w, 0.0); }

/// Whether a backdrop is busy enough that the art needs a dark halo.
fn busy(s: u32) -> bool { return params[s + S_BACKDROP] >= B_JULIA; }

/// Screen position as a point on the complex plane, `scale` high.
fn plane(x: f32, y: f32, centre: vec2<f32>, scale: f32) -> vec2<f32> {
    return centre + vec2((x - 0.5) * pf(H_ASPECT), y - 0.5) * scale;
}

fn smooth_escape(i: f32, z: vec2<f32>) -> f32 { return i + 1.0 - log2(log2(max(dot(z, z), 1.0001)) * 0.5 + 1e-6); }

fn escape_color(s: u32, mu: f32, limit: f32, t: f32) -> vec3<f32> {
    if mu >= limit { return background(s) * 0.6; }
    // Most of the plane escapes in a few steps: a log scale lights the bands
    // near the set instead of leaving them dark.
    let shade = clamp(log2(mu + 1.0) / log2(min(limit, 48.0)), 0.0, 1.0);
    return sample(s, log2(mu + 1.0) * 0.3 + t * 0.03) * (0.12 + 0.88 * shade);
}

fn julia(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    // Along the edge of the Mandelbrot set's main cardioid, where Julia sets
    // stay connected and full of filigree.
    let a = t * 0.07;
    let w = vec2(cos(a), sin(a)) * 0.5;
    let c = (w - vec2(w.x * w.x - w.y * w.y, 2.0 * w.x * w.y)) * 0.985;
    var z = plane(x, y, vec2(0.0), 2.1);
    let limit = 96.0;
    var i = 0.0;
    for (; i < limit; i += 1.0) {
        if dot(z, z) > 16.0 { break; }
        z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + c;
    }
    return escape_color(s, select(smooth_escape(i, z), limit, i >= limit), limit, t);
}

fn orbit(n: u32) -> vec2<f32> {
    let o = params[H_IN_ORBIT] + min(n, max(params[H_ORBIT_LEN], 1u) - 1u) * 2u;
    return vec2(bitcast<f32>(inputs[o]), bitcast<f32>(inputs[o + 1u]));
}

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x); }

/// An endless zoom into seahorse valley, in and back out every few minutes.
/// f32 can't tell neighbouring pixels apart past about 1e-6, so each pixel
/// only follows its small difference from a reference orbit the CPU
/// computed in double precision, and starts over from the reference
/// ("rebases") when the difference would grow larger than the orbit.
fn mandelbrot(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    // Deeper views need more iterations: about a thousand at 1e-8.
    let depth = 18.0 * (0.5 - 0.5 * cos(t * TAU / 120.0));
    let scale = 3.0 * exp(-depth);
    let dc = vec2((x - 0.5) * pf(H_ASPECT), y - 0.5) * scale;
    let limit = min(200.0 + 60.0 * depth + 4.0 * depth * depth, 3000.0);
    var dz = vec2(0.0);
    var m = 0u;
    var i = 0.0;
    var z = vec2(0.0);
    for (; i < limit; i += 1.0) {
        dz = cmul(2.0 * orbit(m) + dz, dz) + dc;
        m += 1u;
        z = orbit(m) + dz;
        if dot(z, z) > 16.0 { break; }
        if dot(z, z) < dot(dz, dz) || m + 1u >= params[H_ORBIT_LEN] {
            dz = z;
            m = 0u;
        }
    }
    return escape_color(s, select(smooth_escape(i, z), limit, i >= limit), limit, t);
}

fn burning_ship(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let scale = 0.16 * exp(-(age(s) % 30.0) * 0.12);
    let c = plane(x, 1.0 - y, vec2(-1.762, -0.028), scale);
    var z = vec2(0.0);
    let limit = 120.0;
    var i = 0.0;
    for (; i < limit; i += 1.0) {
        if dot(z, z) > 16.0 { break; }
        let a = abs(z);
        z = vec2(a.x * a.x - a.y * a.y, -2.0 * a.x * a.y) + c;
    }
    return escape_color(s, select(smooth_escape(i, z), limit, i >= limit), limit, t);
}

fn newton(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    var z = rot(t * 0.07) * plane(x, y, vec2(0.0), 2.4);
    let relax = 1.0 + 0.35 * sin(t * 0.25);
    var n = 0.0;
    for (; n < 32.0; n += 1.0) {
        // z -= relax * (z^3 - 1) / (3 z^2)
        let z2 = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y);
        let z3 = vec2(z2.x * z.x - z2.y * z.y, z2.x * z.y + z2.y * z.x);
        let f = z3 - vec2(1.0, 0.0);
        let df = 3.0 * z2;
        let q = vec2(f.x * df.x + f.y * df.y, f.y * df.x - f.x * df.y) / max(dot(df, df), 1e-9);
        z -= relax * q;
        if dot(f, f) < 1e-6 { break; }
    }
    let root = (atan2(z.y, z.x) / TAU + 1.0) * 3.0;
    let shade = 1.0 - n / 32.0;
    return sample(s, floor(root + 0.5) / 3.0 + t * 0.01) * (0.2 + 0.8 * pow(shade, 1.5));
}

fn kaleidoscope(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let p = vec2((x - 0.5) * pf(H_ASPECT), y - 0.5);
    let segments = 8.0;
    let r = length(p);
    var a = atan2(p.y, p.x) + t * 0.08;
    let wedge = TAU / segments;
    a = abs(a - floor(a / wedge) * wedge - wedge * 0.5);
    let q = vec2(cos(a), sin(a)) * r * 3.0;
    let n = fbm(q + vec2(t * 0.15, -t * 0.1));
    return sample(s, n * 1.5 + r * 0.6 - t * 0.04) * (0.25 + 0.75 * smoothstep(0.25, 0.75, n));
}

fn tunnel(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let p = vec2((x - 0.5) * pf(H_ASPECT), y - 0.5);
    let r = max(length(p), 1e-3);
    let u = atan2(p.y, p.x) / PI + t * 0.05;
    let v = 0.3 / r + t * 0.6;
    let checker = select(0.65, 1.0, (u32(floor(u * 8.0) + floor(v * 4.0)) & 1u) == 0u);
    let n = fbm(vec2(u * 6.0, v * 2.0));
    return sample(s, v * 0.15 + n * 0.3) * checker * smoothstep(0.0, 0.35, r) * 0.9;
}

fn warp_stars(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let p = vec2((x - 0.5) * pf(H_ASPECT), y - 0.5);
    let r = length(p);
    let a = atan2(p.y, p.x);
    var light = vec3(0.0);
    for (var layer = 0u; layer < 3u; layer++) {
        let sectors = 120.0 + f32(layer) * 70.0;
        let sector = floor((a / TAU + 0.5) * sectors);
        let seed = hash(u32(sector) + layer * 977u, 3u);
        let speed = 0.25 + seed * 0.5;
        let depth = wrap(seed * 7.0 + t * speed);
        let star = depth * depth * 0.9;
        let trail = 0.01 + depth * 0.08;
        let along = 1.0 - smoothstep(0.0, trail, abs(r - star));
        let across = abs(wrap((a / TAU + 0.5) * sectors) - 0.5);
        let core = 1.0 - smoothstep(0.05, 0.5, across);
        light += lerp(sample(s, seed), WHITE, 0.5) * along * core * depth;
    }
    return background(s) + light;
}

// ---------------------------------------------------------------- 3D fractals

fn camera_ray(x: f32, y: f32, t: f32, orbit: f32) -> array<vec3<f32>, 2> {
    let a = t * 0.18;
    let ro = vec3(sin(a) * orbit, 0.35 * sin(t * 0.13) * orbit, -cos(a) * orbit);
    let forward = normalize(-ro);
    let right = normalize(cross(vec3(0.0, 1.0, 0.0), forward));
    let up = cross(forward, right);
    let uv = vec2((x - 0.5) * pf(H_ASPECT), 0.5 - y);
    return array<vec3<f32>, 2>(ro, normalize(forward * 1.6 + right * uv.x + up * uv.y));
}

/// Distance to the Mandelbulb, and an orbit trap for colour.
fn bulb(p: vec3<f32>, power: f32) -> vec2<f32> {
    var z = p;
    var dr = 1.0;
    var r = 0.0;
    var trap = 1e9;
    for (var i = 0; i < 6; i++) {
        r = length(z);
        if r > 2.0 { break; }
        trap = min(trap, r);
        let theta = acos(clamp(z.z / r, -1.0, 1.0)) * power;
        let phi = atan2(z.y, z.x) * power;
        dr = pow(r, power - 1.0) * power * dr + 1.0;
        z = pow(r, power) * vec3(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta)) + p;
    }
    return vec2(0.5 * log(max(r, 1e-6)) * r / dr, trap);
}

fn mandelbulb(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let ray = camera_ray(x, y, t, 2.6);
    let power = 8.0 + 1.5 * sin(t * 0.1);
    var d = 0.0;
    var steps = 0.0;
    var trap = 1.0;
    var found = false;
    for (; steps < 72.0; steps += 1.0) {
        let res = bulb(ray[0] + ray[1] * d, power);
        if res.x < 0.0015 * d { found = true; trap = res.y; break; }
        d += res.x;
        if d > 6.0 { break; }
    }
    let occlusion = 1.0 - steps / 72.0;
    if !found { return background(s) + sample(s, 0.6) * pow(1.0 - occlusion, 4.0) * 0.25; }
    return sample(s, trap * 1.3 + t * 0.03) * (0.15 + 0.85 * pow(occlusion, 1.6));
}

fn box_distance(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn menger(p: vec3<f32>) -> f32 {
    var d = box_distance(p, vec3(1.0));
    var scale = 1.0;
    for (var i = 0; i < 4; i++) {
        let a = (p * scale + 3.0 * 1e3) % vec3(2.0) - 1.0;
        scale *= 3.0;
        let r = abs(1.0 - 3.0 * abs(a));
        let da = max(r.x, r.y);
        let db = max(r.y, r.z);
        let dc = max(r.z, r.x);
        d = max(d, (min(da, min(db, dc)) - 1.0) / scale);
    }
    return d;
}

fn menger_sponge(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let uv = vec2((x - 0.5) * pf(H_ASPECT), 0.5 - y);
    let ro = vec3(0.0, 0.0, t * 0.35 - 0.0);
    var rd = normalize(vec3(uv, 1.3));
    let spin = rot(t * 0.07);
    rd = vec3(spin * rd.xy, rd.z);
    var d = 0.0;
    var steps = 0.0;
    var found = false;
    for (; steps < 64.0; steps += 1.0) {
        var p = ro + rd * d;
        p = (p + 1.0) % vec3(2.0) - 1.0;
        let dist = menger(p);
        if dist < 0.0015 * d + 0.0005 { found = true; break; }
        d += dist;
        if d > 8.0 { break; }
    }
    let occlusion = 1.0 - steps / 64.0;
    let fog = exp(-d * 0.35);
    if !found { return background(s); }
    return sample(s, d * 0.12 + t * 0.02) * pow(occlusion, 1.4) * fog + background(s) * (1.0 - fog);
}

// ---------------------------------------------------------------- more 2D and 3D

fn terrain_height(p: vec2<f32>) -> f32 {
    let q = p + vec2(500.0, 500.0);
    return fbm(q * 0.35) * 2.4 + value_noise(q * 1.9) * 0.12;
}

/// Flying low over hills, lit by a low sun, fading into haze.
fn terrain(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let uv = vec2((x - 0.5) * pf(H_ASPECT), 0.5 - y);
    let ro = vec3(0.4 * sin(t * 0.05), 2.4, t * 1.1);
    let rd = normalize(vec3(uv.x, uv.y - 0.22, 1.0));
    let sun = normalize(vec3(-0.5, 0.3, 1.0));
    let sky = mixl(background(s) * 1.4, sample(s, 0.2) * 0.55, clamp(uv.y * 1.5 + 0.35, 0.0, 1.0))
        + sample(s, 0.05) * (pow(max(dot(rd, sun), 0.0), 900.0) * 0.8 + pow(max(dot(rd, sun), 0.0), 12.0) * 0.15);
    var d = 0.3;
    for (var i = 0; i < 64; i++) {
        let p = ro + rd * d;
        let h = terrain_height(p.xz);
        let gap = p.y - h;
        if gap < 0.003 * d {
            let e = 0.06;
            let n = normalize(vec3(terrain_height(p.xz - vec2(e, 0.0)) - terrain_height(p.xz + vec2(e, 0.0)), 2.0 * e,
                terrain_height(p.xz - vec2(0.0, e)) - terrain_height(p.xz + vec2(0.0, e))));
            let lit = sample(s, h * 0.3 + 0.1) * (0.2 + 0.95 * max(dot(n, sun), 0.0));
            return mixl(lit, sky, 1.0 - exp(-d * 0.07));
        }
        d += max(gap * 0.45, 0.02);
        if d > 28.0 { break; }
    }
    return sky;
}

/// Blobs drifting on slow orbits; where their fields add past a threshold
/// they merge, with a bright rim.
fn metaballs(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let aspect = pf(H_ASPECT);
    let p = vec2((x - 0.5) * aspect, y - 0.5);
    var field = 0.0;
    var tint = vec3(0.0);
    for (var k = 0; k < 7; k++) {
        let fk = f32(k);
        let c = vec2(sin(t * 0.21 * (1.0 + fk * 0.13) + fk * 1.7) * 0.42 * aspect, cos(t * 0.17 * (1.0 + fk * 0.11) + fk * 2.3) * 0.36);
        let r = 0.08 + 0.035 * sin(fk * 3.1 + 1.0);
        let f = r * r / max(dot(p - c, p - c), 1e-5);
        field += f;
        tint += sample(s, fk / 7.0 + t * 0.01) * f;
    }
    tint /= max(field, 1e-4);
    let inside = smoothstep(0.9, 1.1, field);
    let rim = exp(-pow((field - 1.0) * 4.0, 2.0));
    return background(s) * 0.7 + tint * (inside * 0.45 + rim * 0.7 + min(field, 1.0) * 0.08);
}

/// Stained glass: cells around drifting points, each its own colour,
/// brightest at the middle, with dark leading between.
fn stained_glass(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let p = vec2((x - 0.5) * pf(H_ASPECT), y - 0.5) * 7.0 + vec2(40.0);
    let g = floor(p);
    let f = p - g;
    var d1 = 9.0;
    var d2 = 9.0;
    var id = vec2(0.0);
    for (var j = -1; j <= 1; j++) {
        for (var i = -1; i <= 1; i++) {
            let cell = g + vec2(f32(i), f32(j));
            let h = vec2(hashf(cell.x, cell.y), hashf(cell.x + 31.0, cell.y + 17.0));
            let site = vec2(f32(i), f32(j)) + 0.5 + 0.38 * sin(t * 0.25 + h * TAU);
            let d = length(site - f);
            if d < d1 { d2 = d1; d1 = d; id = cell; } else if d < d2 { d2 = d; }
        }
    }
    let glass = sample(s, hashf(id.x, id.y) + t * 0.01) * (0.45 + 0.5 * (1.0 - d1));
    return mixl(background(s) * 0.25, glass * 0.8, smoothstep(0.03, 0.09, d2 - d1));
}

/// Marbled paper: noise warped by noise warped by noise.
fn marbling(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let p = vec2((x - 0.5) * pf(H_ASPECT), y - 0.5) * 2.6 + vec2(10.0);
    let q = vec2(fbm(p + vec2(0.0, t * 0.04)), fbm(p + vec2(5.2, 1.3) - vec2(t * 0.03)));
    let r = vec2(fbm(p + 4.0 * q + vec2(1.7, 9.2) + vec2(t * 0.02)), fbm(p + 4.0 * q + vec2(8.3, 2.8)));
    let v = fbm(p + 4.0 * r);
    return sample(s, v * 1.5 + length(q) * 0.6 + t * 0.01) * (0.25 + 0.8 * v * v);
}

/// Distance to the edges of a fractal folded by `fold` and inverted in the
/// unit circle by `k`, and how much it was scaled, in the plane's units.
fn inverted(p0: vec2<f32>, k: f32, kleinian: bool, box: vec2<f32>) -> vec2<f32> {
    var p = p0;
    var scale = 1.0;
    for (var i = 0; i < 10; i++) {
        if kleinian {
            p = 2.0 * clamp(p, -box, box) - p;
        } else {
            p = -1.0 + 2.0 * fract(0.5 * p + 0.5);
        }
        let r2 = max(dot(p, p), 1e-5);
        let f = select(k / r2, max(k / r2, 1.0), kleinian);
        p *= f;
        scale *= f;
    }
    return vec2(0.25 * abs(p.y) / scale, scale);
}

/// The Apollonian gasket (circles in circles), or a pseudo-Kleinian limit
/// set (box folds and inversions), its edges lit, zooming in and back.
fn gasket(s: u32, x: f32, y: f32, t: f32, kleinian: bool) -> vec3<f32> {
    let zoom = exp(-(0.5 - 0.5 * cos(t * TAU / 90.0)) * 3.5);
    let centre = select(vec2(0.3, 0.2), vec2(0.0, 0.62), kleinian);
    let p = centre + vec2((x - 0.5) * pf(H_ASPECT), y - 0.5) * 2.4 * zoom;
    let box = vec2(0.92 + 0.04 * sin(t * 0.07), 0.9);
    let r = inverted(p, select(1.15 + 0.06 * sin(t * 0.11), 1.0, kleinian), kleinian, box);
    let pixel = 2.4 * zoom / f32(grid_h());
    let edge = exp(-r.x / pixel * 0.9);
    let shade = sample(s, log2(r.y) * 0.07 + t * 0.015);
    return background(s) * 0.6 + shade * (edge * 0.95 + 0.12);
}

/// The scene's own art, small, in staggered rows drifting across.
fn tiles(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    if params[s + S_PIECES] == 0u { return background(s); }
    let p = piece(s, 0u);
    let tile_w = width() / 5.0;
    let scale = tile_w / max(p.w, 1.0);
    let span_x = tile_w * 1.3;
    let span_y = p.h * scale * 1.5;
    var q = vec2(x * width() + t * 14.0, y * height() + t * 5.0);
    let row = floor(q.y / span_y);
    q.x += row * span_x * 0.5;
    let local = vec2(wrap(q.x / span_x) * span_x, wrap(q.y / span_y) * span_y) / scale;
    let ink = f32(texel(p, local) & 0xffu) / 255.0;
    return background(s) * 0.8 + sample(s, row * 0.13 + floor(q.x / span_x) * 0.07 + t * 0.02) * ink * 0.3;
}

/// Background at `(x, y)` in 0..1, half-cell pixel `(px, py)`.
fn backdrop(s: u32, x: f32, y: f32, px: u32, py: u32, t: f32) -> vec3<f32> {
    let aspect = pf(H_ASPECT);
    let dx = x - 0.5;
    let dy = y - 0.5;
    let dither = hash(px, py + 7u) * 2.5 / 255.0;
    let base = background(s) * (1.6 - 1.8 * (dx * dx + dy * dy)) + vec3(dither);
    switch params[s + S_BACKDROP] {
        case B_STARS: {
            let seed = hash(px / 2u, py / 4u);
            if seed > 0.006 { return base; }
            let twinkle = sin(((t * (0.5 + seed * 300.0)) + seed * 1000.0) * TAU) * 0.5 + 0.5;
            return base + lerp(WHITE, sample(s, seed * 100.0), 0.4) * pow(twinkle, 3.0) * 0.75;
        }
        case B_GRID: {
            let horizon = 0.58;
            let glow = sample(s, 0.15) * 0.35 * exp(-pow((y - horizon) * 14.0, 2.0));
            if y < horizon { return base + sample(s, 0.75) * 0.12 * pow(y / horizon, 2.0) + glow; }
            let depth = 1.0 / (y - horizon + 0.02);
            let z = depth * 0.6 + t * 1.2;
            let lane = dx * aspect * depth * 0.9;
            let fade = pow((y - horizon) / (1.0 - horizon), 0.6);
            let grid = max(line(z, 0.12), line(lane, 0.08 * min(depth, 10.0) / 4.0));
            return base + sample(s, 0.0) * grid * 0.45 * fade + glow;
        }
        case B_AURORA: {
            let wave = sin(x * 3.0 + t * 0.15) * 0.08 + sin(x * 7.0 - t * 0.1) * 0.04;
            let band = exp(-pow((y - 0.32 - wave) * 6.0, 2.0));
            let rays = sin(x * 40.0 + sin(x * 5.0 + t * 0.3) * 6.0) * 0.35 + 0.65;
            return base + sample(s, x * 0.6 + t * 0.02) * band * rays * 0.28;
        }
        case B_RAYS: {
            let angle = atan2(dy / max(aspect, 0.1), dx) / TAU;
            let beam = abs(wrap(angle * 14.0 + t * 0.04) - 0.5) * 2.0;
            let falloff = max(1.0 - sqrt(dx * dx + dy * dy) * 1.4, 0.0);
            return base + sample(s, 0.3) * beam * beam * falloff * 0.12;
        }
        // Fractals are vivid; dimmed, they let the art lead.
        case B_JULIA: { return julia(s, x, y, t) * 0.7; }
        case B_MANDELBROT: { return mandelbrot(s, x, y, t) * 0.7; }
        case B_SHIP: { return burning_ship(s, x, y, t) * 0.75; }
        case B_NEWTON: { return newton(s, x, y, t) * 0.6; }
        case B_KALEIDOSCOPE: { return kaleidoscope(s, x, y, t) * 0.65; }
        case B_TUNNEL: { return tunnel(s, x, y, t) * 0.5; }
        case B_WARP: { return warp_stars(s, x, y, t); }
        case B_MANDELBULB: { return mandelbulb(s, x, y, t) * 0.75; }
        case B_MENGER: { return menger_sponge(s, x, y, t) * 0.75; }
        case B_TERRAIN: { return terrain(s, x, y, t) * 0.8; }
        case B_METABALLS: { return metaballs(s, x, y, t) * 0.75; }
        case B_VORONOI: { return stained_glass(s, x, y, t) * 0.7; }
        case B_MARBLE: { return marbling(s, x, y, t) * 0.7; }
        case B_APOLLONIAN: { return gasket(s, x, y, t, false) * 0.75; }
        case B_KLEINIAN: { return gasket(s, x, y, t, true) * 0.75; }
        case B_TILES: { return tiles(s, x, y, t); }
        // Flames and attractors are drawn over this from their histogram.
        case B_FLAME, B_CLIFFORD, B_DE_JONG, B_LORENZ, B_PARTICLE_LIFE: { return background(s) * 0.7 + vec3(dither); }
        default: { return base; }
    }
}

/// Whether a backdrop is drawn by splatting points (flames, attractors).
fn splatted(s: u32) -> bool { let b = params[s + S_BACKDROP]; return b >= B_FLAME && b <= B_PARTICLE_LIFE; }

/// Backgrounds for the grid, for the scene to sample: the new scene's
/// always, the old one's while a transition shows it.
@compute @workgroup_size(64)
fn backdrops(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let gx = i % grid_w();
    let gy = i / grid_w();
    let x = (f32(gx) + 0.5) / f32(grid_w());
    let y = (f32(gy) + 0.5) / f32(grid_h());
    let t = time();
    state[params[H_BG_NEW] + i] = vec4(backdrop(H_TO, x, y, gx, gy, t), 1.0);
    if kind() != K_STILL {
        state[params[H_BG_OLD] + i] = vec4(backdrop(H_FROM, x, y, gx, gy, t), 1.0);
    }
}
