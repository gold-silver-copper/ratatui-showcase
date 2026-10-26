// Particles, splatted into `cover` (coverage per sample) and `color`
// (premultiplied colour per half cell) for the scene pass to read.
//
// Transition particles follow scripted paths; swarm particles keep a
// position and velocity in `state` and are pulled toward the art. Their
// places in the art are found at rest (transition.rs) and moved with it.

struct Point { x: f32, y: f32, seed: f32 }

fn point_at(base: u32, i: u32) -> Point {
    let o = params[base] + i * 3u;
    return Point(bitcast<f32>(inputs[o]), bitcast<f32>(inputs[o + 1u]), bitcast<f32>(inputs[o + 2u]));
}

/// Point `i` where scene `s`'s art has moved it to by now.
fn point_now(base: u32, i: u32, s: u32) -> Point {
    let p = point_at(base, i);
    let q = placed(s, p.x, p.y);
    return Point(q.x, q.y, p.seed);
}

fn particle_color(s: u32, p: Point) -> vec3<f32> { return ink_at(s, p.x, p.y, time()); }

/// Draws a square particle of `size` samples centred on `(x, y)`: coverage
/// on every sample it touches, colour on every half cell it touches by how
/// much of it lands there.
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
    if x0 >= x1 || y0 >= y1 { return; }
    let a = u32(clamp(alpha, 0.0, 1.0) * 255.0);
    for (var sy = y0; sy < y1; sy++) {
        for (var sx = x0; sx < x1; sx++) {
            let cell = (sy / ROWS) * cols + sx / COLS;
            atomicMax(&cover[cell * SAMPLES + (sy % ROWS) * COLS + sx % COLS], a);
        }
    }
    // Half cells are COLS x ROWS / 2 samples.
    let tall = ROWS / 2u;
    let premul = clamp(c, vec3(0.0), vec3(1.0)) * f32(a);
    for (var hy = y0 / tall; hy <= (y1 - 1u) / tall; hy++) {
        let span_y = min(y1, (hy + 1u) * tall) - max(y0, hy * tall);
        for (var hx = x0 / COLS; hx <= (x1 - 1u) / COLS; hx++) {
            let span = f32((min(x1, (hx + 1u) * COLS) - max(x0, hx * COLS)) * span_y);
            let slot = (((hy / 2u) * cols + hx) * 2u + hy % 2u) * 4u;
            atomicAdd(&color[slot], u32(premul.r * span));
            atomicAdd(&color[slot + 1u], u32(premul.g * span));
            atomicAdd(&color[slot + 2u], u32(premul.b * span));
            atomicAdd(&color[slot + 3u], u32(f32(a) * span));
        }
    }
}

@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let cell = thread(id, groups);
    if cell >= params[H_COLS] * params[H_ROWS] { return; }
    for (var i = 0u; i < SAMPLES; i++) { atomicStore(&cover[cell * SAMPLES + i], 0u); }
    for (var i = 0u; i < 8u; i++) { atomicStore(&color[cell * 8u + i], 0u); }
}

fn is_particles(k: u32) -> bool { return k >= K_MORPH && k <= K_CRUMBLE; }

@compute @workgroup_size(64)
fn particles(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let k = kind();
    if !is_particles(k) { return; }
    let i = thread(id, groups);
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
        let a = point_now(H_IN_POINTS, i * ns / n, H_FROM);
        let b = point_now(H_IN_POINTS, ns + i * nt / n, H_TO);
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
    let layer = select(H_TO, H_FROM, source);
    let pt = point_now(H_IN_POINTS, i, layer);
    switch k {
        case K_SCATTER: {
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
        case K_POUR, K_CRUMBLE: {
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
        case K_VORTEX: {
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
/// by curl-noise currents, and every few seconds thrown out in a burst. They
/// belong to the new scene, or to the old one while it fades.
@compute @workgroup_size(64)
fn swarm(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    let homes = params[H_HOMES];
    if i >= params[H_SWARM] || homes == 0u { return; }
    let slot = params[H_PARTICLES] + i;
    var particle = state[slot];
    let w = width();
    let h = height();
    if particle.x == 0.0 && particle.y == 0.0 {
        particle = vec4(hash(i, 21u) * w, hash(i, 22u) * h, 0.0, 0.0);
    }
    let s = params[H_SWARM_SCENE];
    let home = point_now(H_IN_HOMES, i % homes, s);
    var pos = particle.xy;
    var vel = particle.zw;
    let t = time();
    let delta = dt();
    let n = fbm(pos * 0.015 + vec2(t * 0.1, home.seed * 3.0));
    let curl = vec2(cos(n * TAU * 2.0), sin(n * TAU * 2.0));
    let burst = smoothstep(0.92, 1.0, wrap(age(s) / 6.0));
    let centre = vec2(w, h) / 2.0;
    let out = normalize(pos - centre + vec2(0.01)) * burst * 900.0;
    let force = (vec2(home.x, home.y) - pos) * 7.0 - vel * 3.2 + curl * 60.0 + out;
    vel += force * delta;
    pos += vel * delta;
    state[slot] = vec4(pos, vel);
    let speed = length(vel);
    let c = lerp(ink_at(s, home.x, home.y, t), WHITE, clamp(speed / 900.0, 0.0, 0.6));
    // The old scene's swarm goes as its art does.
    let alpha = select(1.0, opacity(0u, pos.x, pos.y), s == H_FROM && kind() != K_STILL);
    splat(pos.x, pos.y, 2.0, c, alpha);
}

// ---------------------------------------------------------------- flames and attractors
//
// Each thread carries one point (xy, colour index, random state) between
// frames and moves it a few steps per frame, adding every position it visits
// to a histogram that fades a little each frame (by H_FADE, which keeps the
// fade per second the same at any frame rate). Millions of points per frame
// build up the fine structure; tone mapping in the scene pass turns hit
// counts into light. A different backdrop starts the points and the
// histogram over.

/// Whether the points and histogram belong to another backdrop.
fn splat_changed() -> bool { return mark(MARK_SPLAT) != params[H_SPLAT_ID]; }

fn hist_index(x: f32, y: f32) -> i32 {
    // Written so NaN fails it too.
    if !(x >= 0.0 && y >= 0.0 && x < width() && y < height()) { return -1; }
    return i32((u32(y) * img_w() + u32(x)) * 2u);
}

fn hist_add(p: vec2<f32>, c: f32) {
    let i = hist_index(p.x, p.y);
    if i < 0 { return; }
    atomicAdd(&hist[u32(i)], 1u);
    atomicAdd(&hist[u32(i) + 1u], u32(clamp(c, 0.0, 1.0) * 255.0));
}

@compute @workgroup_size(64)
fn splat_fade(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= img_w() * img_h() { return; }
    let fade = select(pf(H_FADE), 0.0, splat_changed());
    let count = atomicLoad(&hist[i * 2u]);
    let index = atomicLoad(&hist[i * 2u + 1u]);
    atomicStore(&hist[i * 2u], u32(f32(count) * fade));
    atomicStore(&hist[i * 2u + 1u], u32(f32(index) * fade));
}

fn next_random(state: ptr<function, u32>) -> f32 {
    var x = *state;
    x ^= x << 13u;
    x ^= x >> 17u;
    x ^= x << 5u;
    *state = x;
    return f32(x >> 8u) / 16777216.0;
}

/// One of the classic flame variations.
fn variation(v: u32, p: vec2<f32>) -> vec2<f32> {
    let r2 = max(dot(p, p), 1e-6);
    let r = sqrt(r2);
    let theta = atan2(p.x, p.y);
    switch v % 8u {
        case 0u: { return p; }
        // Eyefish, rather than sinusoidal: sine folds every point into a
        // square, which reads as a box drawn around the art.
        case 1u: { return p * 2.0 / (r + 1.0); }
        case 2u: { return p / r2; }
        case 3u: { return vec2(p.x * sin(r2) - p.y * cos(r2), p.x * cos(r2) + p.y * sin(r2)); }
        case 4u: { return vec2((p.x - p.y) * (p.x + p.y), 2.0 * p.x * p.y) / r; }
        case 5u: { return vec2(theta / PI, r - 1.0); }
        case 6u: { return r * vec2(sin(theta * r), -cos(theta * r)); }
        default: { return theta / PI * vec2(sin(PI * r), cos(PI * r)); }
    }
}

/// Which scene's backdrop is splatted this frame: the new one's, else the old.
fn splat_scene() -> u32 { return select(H_FROM, H_TO, splatted(H_TO)); }

@compute @workgroup_size(64)
fn flame(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= params[H_FLAME_THREADS] { return; }
    let s = splat_scene();
    let kind_of = params[s + S_BACKDROP];
    let slot = params[H_FLAME] + i;
    var st = state[slot];
    var rng = bitcast<u32>(st.w);
    if rng == 0u || splat_changed() {
        rng = (i + 1u) * 747796405u + 2891336453u;
        st = vec4(next_random(&rng) * 2.0 - 1.0, next_random(&rng) * 2.0 - 1.0, next_random(&rng), 0.0);
        if kind_of == B_LORENZ { st = vec4(st.x * 10.0, st.y * 10.0, 20.0 + st.z * 10.0, 0.0); }
    }
    let t = time() * 0.6;
    let v = variant(s);
    let centre = vec2(width(), height()) / 2.0;
    let unit = min(width() / pf(H_ASPECT), height());
    var p = st.xy;
    var c = st.z;
    for (var k = 0u; k < params[H_FLAME_STEPS]; k++) {
        var screen = vec2(0.0);
        switch kind_of {
            case B_FLAME: {
                // A fractal flame: three drifting transforms, each warped by a variation.
                let r = next_random(&rng);
                let j = u32(r * 3.0);
                let fj = f32(j);
                let angle = fj * 2.094 + t * 0.05 * (fj + 1.0) * select(1.0, -1.0, j == 1u);
                let scale = 0.55 + 0.15 * sin(t * 0.07 + fj);
                let offset = 0.6 * vec2(cos(fj * 2.094 + t * 0.03), sin(fj * 2.094 + t * 0.03));
                let q = rot(angle) * p * scale + offset;
                p = mix(q, variation(v * 3u + j, q), 0.7);
                c = (c + fj / 3.0 + 0.1) * 0.5;
                screen = centre + p * unit * 0.4;
            }
            case B_COLLAGE: {
                // Barnsley's collage theorem: maps that each shrink the art
                // onto one inked cell of a grid laid over it have the art as
                // their attractor, so it comes out made of copies of itself,
                // made of copies in turn. Each copy turns a little.
                let art = piece(s, 0u);
                let n = 5.0 + f32(v % 3u);
                let size = vec2(art.w, art.h);
                var cell = vec2(0.0);
                for (var tries = 0; tries < 16; tries++) {
                    cell = floor(vec2(next_random(&rng), next_random(&rng)) * n);
                    if (texel(art, (cell + 0.5) / n * size) & 0xffu) >= 128u { break; }
                }
                let id = hash(u32(cell.x) + v * 17u, u32(cell.y));
                // Turned in the art's own proportions, so copies keep them.
                let aspect = vec2(size.x / max(size.y, 1.0), 1.0);
                let q = rot(0.3 * sin(t * 0.25 + id * TAU)) * ((p - 0.5) * aspect) / aspect;
                p = (cell + 0.5 + q) / n;
                c = mix(c, id, 0.5);
                // Slowly in toward one of the copies and back out.
                var focus = vec2(0.5);
                for (var tries = 0u; tries < 16u; tries++) {
                    let f = vec2(hash(v, tries), hash(v + 7u, tries));
                    if (texel(art, f * size) & 0xffu) >= 128u { focus = f; break; }
                }
                let zoom = exp(1.2 * (0.5 - 0.5 * cos(t * TAU / 40.0)));
                let k = min(width() / max(size.x, 1.0), height() / max(size.y, 1.0)) * 1.1;
                screen = centre + ((p - focus) * zoom + focus - 0.5) * size * k;

            }
            case B_CLIFFORD: {
                // Clifford attractor.
                let a = -1.7 + 0.15 * sin(t * 0.05);
                let b = 1.3 + 0.1 * cos(t * 0.04);
                let cc = -0.1 + 0.2 * sin(t * 0.03);
                let d = -1.21 + 0.1 * cos(t * 0.06);
                let q = vec2(sin(a * p.y) + cc * cos(a * p.x), sin(b * p.x) + d * cos(b * p.y));
                c = clamp(length(q - p) * 0.35, 0.0, 1.0);
                p = q;
                screen = centre + p * unit * 0.3;
            }
            case B_DE_JONG: {
                // De Jong attractor.
                let a = 1.641 + 0.2 * sin(t * 0.04);
                let b = 1.902 + 0.2 * cos(t * 0.05);
                let cc = 0.316 + 0.3 * sin(t * 0.03);
                let d = 1.525 + 0.2 * cos(t * 0.035);
                let q = vec2(sin(a * p.y) - cos(b * p.x), sin(cc * p.x) - cos(d * p.y));
                c = clamp(length(q - p) * 0.3, 0.0, 1.0);
                p = q;
                screen = centre + p * unit * 0.33;
            }
            default: {
                // The Lorenz butterfly, turning slowly.
                var x = vec3(p, st.z);
                let h = 0.004;
                let velocity = vec3(10.0 * (x.y - x.x), x.x * (28.0 - x.z) - x.y, x.x * x.y - 8.0 / 3.0 * x.z);
                x += velocity * h;
                p = x.xy;
                st.z = x.z;
                c = clamp(length(velocity) / 180.0, 0.0, 1.0);
                let a = t * 0.15;
                let view = vec2(x.x * cos(a) - x.y * sin(a), -(x.z - 25.0));
                screen = centre + view * unit / 36.0;
            }
        }
        hist_add(screen, c);
    }
    if kind_of == B_LORENZ { st = vec4(p, st.z, bitcast<f32>(rng)); } else { st = vec4(p, c, bitcast<f32>(rng)); }
    // A point that escapes to infinity starts over.
    if any(abs(st.xyz) > vec3(1e4)) || any(st.xyz != st.xyz) { st = vec4(0.0); }
    state[slot] = st;
}

/// The splatted backdrop's light at sample `(x, y)`.
fn splat_light(s: u32, x: f32, y: f32) -> vec3<f32> {
    let i = hist_index(x, y);
    if i < 0 { return vec3(0.0); }
    let count = f32(atomicLoad(&hist[u32(i)]));
    if count <= 0.0 { return vec3(0.0); }
    let index = f32(atomicLoad(&hist[u32(i) + 1u])) / (count * 255.0);
    // Exposure from how many points land per frame, spread over the screen.
    let per_frame = f32(params[H_FLAME_THREADS] * params[H_FLAME_STEPS]);
    let typical = per_frame / max(1.0 - pf(H_FADE), 0.01) / (width() * height()) * 3.0;
    let light = log(1.0 + count) / log(1.0 + typical * 6.0);
    return sample(s, index * 0.85 + time() * 0.02) * clamp(light, 0.0, 1.4);
}

// ---------------------------------------------------------------- particle life
//
// Four kinds of particle, each drawn to or pushed from every other kind by a
// rule the scene's variant picks. A few thousand particles, each feeling
// every other: forces in one pass (velocities only), moves in the next, so
// no particle reads a position while it changes. They share the flame
// points' state and histogram.

@compute @workgroup_size(64)
fn bins_clear(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= bins_w() * bins_h() * BIN_WORDS { return; }
    atomicStore(&bins[i], 0u);
}

/// How kind `a` feels kind `b`: -1 flees, 1 chases.
fn affinity(a: u32, b: u32) -> f32 { return hash(variant(splat_scene()) * 16u + a * 4u + b, 77u) * 2.0 - 1.0; }

/// Particle `i`, started somewhere random when its backdrop is new.
fn life_particle(i: u32) -> vec4<f32> {
    let st = state[params[H_FLAME] + i];
    if splat_changed() || (st.x == 0.0 && st.y == 0.0) {
        return vec4(hash(i, 41u) * width(), hash(i, 42u) * height(), 0.0, 0.0);
    }
    return st;
}

@compute @workgroup_size(64)
fn life_forces(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    let n = params[H_FLAME_THREADS];
    if i >= n { return; }
    let size = vec2(width(), height());
    let reach = min(size.x, size.y) * 0.12;
    let me = life_particle(i);
    let kind = i % 4u;
    var force = vec2(0.0);
    for (var j = 0u; j < n; j++) {
        if j == i { continue; }
        // The screen wraps around: the nearest copy of the other particle.
        var d = life_particle(j).xy - me.xy;
        d -= size * round(d / size);
        let r = length(d) / reach;
        if r >= 1.0 || r < 1e-4 { continue; }
        // Too close pushes apart whatever the kinds; further out the rule
        // pulls or pushes, strongest halfway.
        let pull = select(affinity(kind, j % 4u) * (1.0 - abs(2.0 * r - 1.3) / 0.7), r / 0.3 - 1.0, r < 0.3);
        force += d / (r * reach) * pull;
    }
    // Friction, then the forces, per sixtieth of a second.
    let vel = me.zw * 0.7 + force * reach * 0.012;
    state[params[H_FLAME] + i] = vec4(me.xy, vel);
}

@compute @workgroup_size(64)
fn life_move(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= params[H_FLAME_THREADS] { return; }
    let size = vec2(width(), height());
    let st = state[params[H_FLAME] + i];
    let pos = (st.xy + st.zw + size) % size;
    state[params[H_FLAME] + i] = vec4(pos, st.zw);
    // A small dot, four samples.
    let c = f32(i % 4u) * 0.25 + 0.1;
    for (var k = 0u; k < 4u; k++) {
        hist_add(pos + vec2(f32(k % 2u), f32(k / 2u)), c);
    }
}

// ---------------------------------------------------------------- boids
//
// A flock: each boid steers toward its neighbours' middle, matches their
// heading and keeps its distance (from the bins around it, so no boid visits
// every other), while a home in the art draws the flock into the letters.
// Every so often the pull lets go and the flock wheels away, then returns.

@compute @workgroup_size(64)
fn boid_bin(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= params[H_SWARM] { return; }
    let b = state[params[H_PARTICLES] + i];
    if b.x == 0.0 && b.y == 0.0 { return; }
    let cell = bin_of(b.xy);
    let o = bin_at(cell.x, cell.y) + BIN_BOIDS;
    atomicAdd(&bins[o], 1u);
    atomicAdd(&bins[o + 1u], bitcast<u32>(i32(clamp(b.z, -2000.0, 2000.0) * 16.0)));
    atomicAdd(&bins[o + 2u], bitcast<u32>(i32(clamp(b.w, -2000.0, 2000.0) * 16.0)));
    atomicAdd(&bins[o + 3u], u32(clamp(b.x, 0.0, width()) * 2.0));
    atomicAdd(&bins[o + 4u], u32(clamp(b.y, 0.0, height()) * 2.0));
}

@compute @workgroup_size(64)
fn boids(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    let homes = params[H_HOMES];
    if i >= params[H_SWARM] || homes == 0u { return; }
    let slot = params[H_PARTICLES] + i;
    var b = state[slot];
    let w = width();
    let h = height();
    if b.x == 0.0 && b.y == 0.0 {
        let a = hash(i, 23u) * TAU;
        b = vec4(hash(i, 21u) * w, hash(i, 22u) * h, cos(a) * 80.0, sin(a) * 80.0);
    }
    let s = params[H_SWARM_SCENE];
    let home = point_now(H_IN_HOMES, i % homes, s);
    var pos = b.xy;
    var vel = b.zw;
    let cell = bin_of(pos);
    var count = 0.0;
    var heading = vec2(0.0);
    var centre = vec2(0.0);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            if cell.x + dx < 0 || cell.y + dy < 0 || cell.x + dx >= i32(bins_w()) || cell.y + dy >= i32(bins_h()) { continue; }
            let o = bin_at(cell.x + dx, cell.y + dy) + BIN_BOIDS;
            let n = f32(atomicLoad(&bins[o]));
            count += n;
            heading += vec2(f32(bitcast<i32>(atomicLoad(&bins[o + 1u]))), f32(bitcast<i32>(atomicLoad(&bins[o + 2u])))) / 16.0;
            centre += vec2(f32(atomicLoad(&bins[o + 3u])), f32(atomicLoad(&bins[o + 4u]))) / 2.0;
        }
    }
    var force = vec2(0.0);
    if count > 1.0 {
        let middle = centre / count;
        force += (middle - pos) * 0.9;
        force += (heading / count - vel) * 1.6;
        // Crowded: spread out.
        force -= (middle - pos) * clamp((count - 30.0) / 40.0, 0.0, 2.0) * 2.5;
    }
    // Home, except while the flock is let go.
    let free = smoothstep(0.55, 0.62, wrap(age(s) / 9.0)) * (1.0 - smoothstep(0.85, 0.95, wrap(age(s) / 9.0)));
    force += (vec2(home.x, home.y) - pos) * 3.0 * (1.0 - free);
    // Walls push back.
    force += vec2(select(0.0, 400.0, pos.x < 20.0) - select(0.0, 400.0, pos.x > w - 20.0),
        select(0.0, 400.0, pos.y < 20.0) - select(0.0, 400.0, pos.y > h - 20.0));
    let delta = dt();
    vel += force * delta;
    let speed = length(vel);
    vel *= clamp(speed, 40.0, 320.0) / max(speed, 1e-3);
    pos += vel * delta;
    state[slot] = vec4(pos, vel);
    let c = lerp(ink_at(s, home.x, home.y, time()), WHITE, clamp(speed / 900.0, 0.0, 0.4));
    let alpha = select(1.0, opacity(0u, pos.x, pos.y), s == H_FROM && kind() != K_STILL);
    splat(pos.x, pos.y, 2.0, c, alpha);
}
