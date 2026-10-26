// Particles, splatted into `cover` (coverage per sample) and `color`
// (premultiplied colour per half cell) for the scene pass to read.
//
// Transition particles follow scripted paths; swarm particles keep a
// position and velocity in `state` and are pulled toward the art.

struct Point { x: f32, y: f32, seed: f32 }

fn point_at(base: u32, i: u32) -> Point {
    let o = params[base] + i * 3u;
    return Point(bitcast<f32>(inputs[o]), bitcast<f32>(inputs[o + 1u]), bitcast<f32>(inputs[o + 2u]));
}

fn particle_color(s: u32, p: Point) -> vec3<f32> { return ink_at(s, p.x, p.y, time()); }

/// Draws a square particle of `size` samples centred on `(x, y)`.
fn splat(x: f32, y: f32, size: f32, c: vec3<f32>, alpha: f32) {
    if alpha <= 0.0 { return; }
    let cols = params[H_COLS];
    let w = img_w();
    let h = img_h();
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

fn is_particles(k: u32) -> bool { return k >= K_MORPH && k <= K_CRUMBLE; }

@compute @workgroup_size(64)
fn particles(@builtin(global_invocation_id) id: vec3<u32>) {
    let k = kind();
    if !is_particles(k) { return; }
    let i = id.x;
    let ns = params[H_SOURCES];
    let nt = params[H_TARGETS];
    let p = progress();
    let grain = pf(H_GRAIN);
    let w = width();
    let h = height();
    let c = vec2(w, h) / 2.0;

    if k == K_MORPH || k == K_SWARM {
        let n = max(ns, nt);
        if i >= n || ns == 0u || nt == 0u { return; }
        let a = point_at(H_IN_POINTS, i * ns / n);
        let b = point_at(H_IN_POINTS, ns + i * nt / n);
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
    let pt = point_at(H_IN_POINTS, i);
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

/// Swarm particles: each is pulled toward a home in the art, pushed around
/// by curl-noise currents, and every few seconds thrown out in a burst.
@compute @workgroup_size(64)
fn swarm(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let homes = params[H_HOMES];
    if i >= params[H_SWARM] || homes == 0u { return; }
    let slot = params[H_PARTICLES] + i;
    var particle = state[slot];
    let w = width();
    let h = height();
    if reset() && particle.x == 0.0 && particle.y == 0.0 {
        particle = vec4(hash(i, 21u) * w, hash(i, 22u) * h, 0.0, 0.0);
    }
    let home = point_at(H_IN_HOMES, i % homes);
    var pos = particle.xy;
    var vel = particle.zw;
    let t = time();
    let delta = dt();
    let n = fbm(pos * 0.015 + vec2(t * 0.1, home.seed * 3.0));
    let curl = vec2(cos(n * TAU * 2.0), sin(n * TAU * 2.0));
    let burst = smoothstep(0.92, 1.0, wrap(age(H_TO) / 6.0)) + bass() * 0.6;
    let centre = vec2(w, h) / 2.0;
    let out = normalize(pos - centre + vec2(0.01)) * burst * 900.0;
    let force = (vec2(home.x, home.y) - pos) * 7.0 - vel * 3.2 + curl * (60.0 + level() * 300.0) + out;
    vel += force * delta;
    pos += vel * delta;
    state[slot] = vec4(pos, vel);
    let speed = length(vel);
    let c = lerp(ink_at(H_TO, home.x, home.y, t), WHITE, clamp(speed / 900.0, 0.0, 0.6));
    splat(pos.x, pos.y, 2.0, c, 1.0);
}
