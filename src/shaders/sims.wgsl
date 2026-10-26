// Simulations that live on the grid between frames, driven by the new
// scene's art. They ping-pong between buffers so no pass reads what another
// thread of the same pass writes.
//
// Fluid: SIM0 velocity (and, mid-frame, pressure and divergence), SIM1 the
// advected velocity and the pressure partner, SIM2/SIM3 dye. The others use
// SIM0/SIM1 and report the latest in H_SIM_LATEST.

fn grid_point(i: u32) -> vec2<f32> { return vec2(f32(i % grid_w()) * 2.0 + 1.0, f32(i / grid_w()) * 2.0 + 1.0); }
fn reset() -> bool { return params[H_SIM_RESET] == 1u; }
fn at(base: u32, i: u32) -> u32 { return params[base] + i; }
fn inside_new(p: vec2<f32>) -> bool { return layer_coverage(1u, p.x, p.y) > 0.5; }

/// The new scene's ink colour at `(x, y)`.
fn ink_at(s: u32, x: f32, y: f32, t: f32) -> vec3<f32> {
    let h = hit(s, x, y, t);
    let uv = h.l / vec2(max(h.p.w, 1.0), max(h.p.h, 1.0));
    return material(s, h.px, shade_ink(s, uv.x, clamp(uv.y, 0.0, 1.0), t, u32(y / 4.0)), t);
}

fn neighbours(base: u32, gx: i32, gy: i32) -> array<vec4<f32>, 4> {
    return array<vec4<f32>, 4>(grid_load(base, gx - 1, gy), grid_load(base, gx + 1, gy), grid_load(base, gx, gy - 1),
        grid_load(base, gx, gy + 1));
}

// ---------------------------------------------------------------- fluid

@compute @workgroup_size(64)
fn fluid_advect(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    if reset() {
        state[at(H_SIM1, i)] = vec4(0.0);
        state[at(H_SIM3, i)] = vec4(0.0);
        return;
    }
    let p = grid_point(i);
    let t = time();
    let v = state[at(H_SIM0, i)].xy;
    let back = p - v * dt() * 2.0;
    var velocity = grid_sample(H_SIM0, back).xy * 0.994;
    var dye = grid_sample(H_SIM2, back).rgb * 0.982;
    // Eddies: the curl of drifting noise, which swirls without pushing the
    // dye anywhere in particular. The music stirs harder.
    let q = p * 0.025 + vec2(t * 0.07, -t * 0.05);
    let e = 0.5;
    let dx = fbm(q + vec2(e, 0.0)) - fbm(q - vec2(e, 0.0));
    let dy = fbm(q + vec2(0.0, e)) - fbm(q - vec2(0.0, e));
    velocity += vec2(dy, -dx) * (240.0 + bass() * 900.0) * dt();
    if inside_new(p) {
        dye = mix(dye, ink_at(H_TO, p.x, p.y, t) * 0.85, 0.1);
    }
    state[at(H_SIM1, i)] = vec4(velocity, 0.0, 0.0);
    state[at(H_SIM3, i)] = vec4(dye, 1.0);
}

@compute @workgroup_size(64)
fn fluid_divergence(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let n = neighbours(H_SIM1, gx, gy);
    let divergence = 0.5 * ((n[1].x - n[0].x) + (n[3].y - n[2].y));
    state[at(H_SIM0, i)] = vec4(0.0, 0.0, 0.0, divergence);
}

@compute @workgroup_size(64)
fn fluid_pressure(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let src = select(H_SIM0, H_SIM1, FLIP == 1u);
    let n = neighbours(src, gx, gy);
    let divergence = state[at(H_SIM0, i)].w;
    let pressure = (n[0].z + n[1].z + n[2].z + n[3].z - divergence) * 0.25;
    if FLIP == 0u { state[at(H_SIM1, i)].z = pressure; } else { state[at(H_SIM0, i)].z = pressure; }
}

@compute @workgroup_size(64)
fn fluid_project(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let n = neighbours(H_SIM0, gx, gy);
    let gradient = 0.5 * vec2(n[1].z - n[0].z, n[3].z - n[2].z);
    let velocity = state[at(H_SIM1, i)].xy - gradient;
    state[at(H_SIM1, i)] = vec4(velocity, 0.0, 0.0);
}

@compute @workgroup_size(64)
fn fluid_copy(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    state[at(H_SIM0, i)] = vec4(state[at(H_SIM1, i)].xy, 0.0, 0.0);
    state[at(H_SIM2, i)] = state[at(H_SIM3, i)];
}

// ---------------------------------------------------------------- reaction-diffusion

@compute @workgroup_size(64)
fn reaction(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let src = select(H_SIM0, H_SIM1, FLIP == 1u);
    let dst = select(H_SIM1, H_SIM0, FLIP == 1u);
    let p = grid_point(i);
    let inside = inside_new(p);
    if reset() {
        let seed = select(0.0, 0.5, inside && hash(i, 31u) < 0.12);
        state[at(dst, i)] = vec4(1.0, seed, 0.0, 0.0);
        return;
    }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let c = grid_load(src, gx, gy).xy;
    var lap = -c;
    let n = neighbours(src, gx, gy);
    for (var k = 0; k < 4; k++) { lap += n[k].xy * 0.2; }
    lap += (grid_load(src, gx - 1, gy - 1).xy + grid_load(src, gx + 1, gy - 1).xy + grid_load(src, gx - 1, gy + 1).xy
        + grid_load(src, gx + 1, gy + 1).xy) * 0.05;
    // Coral grows inside the letters and dies away outside them.
    let feed = select(0.02, 0.0545, inside);
    let kill = select(0.08, 0.062, inside);
    let uvv = c.x * c.y * c.y;
    var u = c.x + (1.0 * lap.x - uvv + feed * (1.0 - c.x));
    var v = c.y + (0.5 * lap.y + uvv - (feed + kill) * c.y);
    if inside && hash3(i, frame(), 7u) < 0.0004 { v = 0.5; }
    state[at(dst, i)] = vec4(clamp(u, 0.0, 1.0), clamp(v, 0.0, 1.0), 0.0, 0.0);
}

// ---------------------------------------------------------------- life

@compute @workgroup_size(64)
fn life(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let src = select(H_SIM0, H_SIM1, FLIP == 1u);
    let dst = select(H_SIM1, H_SIM0, FLIP == 1u);
    let p = grid_point(i);
    let inside = inside_new(p);
    if reset() {
        state[at(dst, i)] = vec4(select(0.0, 1.0, inside && hash(i, 5u) < 0.5), 0.0, 0.0, 0.0);
        return;
    }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    var count = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            if dx == 0 && dy == 0 { continue; }
            let x = (gx + dx + i32(grid_w())) % i32(grid_w());
            let y = (gy + dy + i32(grid_h())) % i32(grid_h());
            count += grid_load(src, x, y).x;
        }
    }
    let here = grid_load(src, gx, gy);
    var alive = select(0.0, 1.0, count == 3.0 || (here.x > 0.5 && count == 2.0));
    let chance = hash3(i, frame(), 11u);
    if (inside && chance < 0.03) || chance < 0.0002 { alive = 1.0; }
    let trail = select(here.y * 0.8, 1.0, alive > 0.5);
    state[at(dst, i)] = vec4(alive, trail, 0.0, 0.0);
}

// ---------------------------------------------------------------- fire

@compute @workgroup_size(64)
fn fire(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let src = select(H_SIM0, H_SIM1, FLIP == 1u);
    let dst = select(H_SIM1, H_SIM0, FLIP == 1u);
    if reset() { state[at(dst, i)] = vec4(0.0); return; }
    let p = grid_point(i);
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let r = hash3(i, frame(), 3u);
    var heat: f32;
    if inside_new(p) {
        heat = 0.8 + 0.2 * r + bass() * 0.3;
    } else if gy == i32(grid_h()) - 1 {
        heat = 0.45 * r;
    } else {
        let drift = i32(floor(r * 3.0)) - 1;
        heat = grid_load(src, gx + drift, gy + 1).x - (0.008 + r * 0.03);
    }
    state[at(dst, i)] = vec4(max(heat, 0.0), 0.0, 0.0, 0.0);
}

// ---------------------------------------------------------------- ripples

@compute @workgroup_size(64)
fn ripple(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let src = select(H_SIM0, H_SIM1, FLIP == 1u);
    let dst = select(H_SIM1, H_SIM0, FLIP == 1u);
    if reset() { state[at(dst, i)] = vec4(0.0); return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let here = grid_load(src, gx, gy);
    let n = neighbours(src, gx, gy);
    let lap = n[0].x + n[1].x + n[2].x + n[3].x - 4.0 * here.x;
    var h = (2.0 * here.x - here.y + 0.25 * lap) * 0.996;
    // Rain: a few drops each second, more with the music.
    let drop = hash3(u32(gx / 4), u32(gy / 4) * 7919u, frame());
    if drop < 0.000008 + bass() * 0.0001 { h += 8.0; }
    state[at(dst, i)] = vec4(h, here.x, 0.0, 0.0);
}

// ---------------------------------------------------------------- display

fn sim_latest() -> u32 { return select(H_SIM0, H_SIM1, params[H_SIM_LATEST] == 1u); }

fn fire_color(s: u32, heat: f32) -> vec3<f32> {
    let h = clamp(heat, 0.0, 1.0);
    let flame = mixl(mixl(mixl(hex(0x200000u), hex(0xd42000u), h * 2.2), hex(0xffb020u), (h - 0.45) * 2.5), WHITE, (h - 0.85) * 5.0);
    return mixl(flame, sample(s, h * 0.6), 0.2);
}

/// The new scene drawn by its simulation at `(x, y)`, over `base`.
fn sim_art(s: u32, x: f32, y: f32, t: f32, row: u32) -> Art {
    var out = Art(vec3(0.0), 0.0, vec3(0.0), 0.0);
    let p = vec2(x, y);
    switch mode(s) {
        case 2u: {
            let dye = grid_sample(H_SIM2, p).rgb;
            let strength = max(dye.r, max(dye.g, dye.b));
            out.alpha = clamp(strength * 1.6, 0.0, 1.0);
            out.color = dye / max(strength, 0.001) * min(strength * 1.3, 1.0);
        }
        case 3u: {
            let v = grid_sample(sim_latest(), p).y;
            let ink = ink_at(s, x, y, t);
            out.alpha = smoothstep(0.12, 0.3, v);
            out.color = mixl(ink * 0.6, lerp(ink, WHITE, 0.3), smoothstep(0.2, 0.4, v));
            out.glow = ink * smoothstep(0.05, 0.2, v) * 0.15;
        }
        case 4u: {
            let cell = grid_load(sim_latest(), i32(x / 2.0), i32(y / 2.0));
            let ink = ink_at(s, x, y, t);
            out.alpha = max(cell.x, cell.y * 0.55);
            out.color = select(ink * 0.7, lerp(ink, WHITE, 0.2), cell.x > 0.5);
        }
        case 5u: {
            let heat = grid_sample(sim_latest(), p).x;
            let a = art(s, 1u, x, y, t, row);
            out.alpha = max(clamp(heat * 1.8, 0.0, 1.0), a.alpha);
            let ember = mixl(background(s) * 0.5, a.color * 0.35, 0.6);
            out.color = select(fire_color(s, heat), ember, a.alpha > 0.5);
            out.glow = fire_color(s, heat) * heat * 0.25;
        }
        default: {}
    }
    return out;
}

/// The water's surface offset at `p`, for refracting everything below it.
fn ripple_offset(p: vec2<f32>) -> vec2<f32> {
    let gx = i32(p.x / 2.0);
    let gy = i32(p.y / 2.0);
    let n = neighbours(sim_latest(), gx, gy);
    return vec2(n[1].x - n[0].x, n[3].x - n[2].x);
}
