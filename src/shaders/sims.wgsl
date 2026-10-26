// Simulations that live on the grid between frames, driven by the art of
// the scene they belong to (H_SIM_SCENE): the new scene's, or the old one's
// while a scene that doesn't simulate takes over. They step at a fixed rate
// (gpu.rs), and ping-pong between buffers so no pass reads what another
// thread of the same pass writes.
//
// Fluid: SIM0 velocity (and, mid-frame, pressure and divergence), SIM1 the
// advected velocity and the pressure partner, SIM2/SIM3 dye. The others use
// SIM0/SIM1: step k of a frame (FLIP k % 2, STEP k) reads the one the steps
// so far (MARK_PARITY) say is current.

/// Seconds per simulation step.
const SIM_DT: f32 = 1.0 / 60.0;

fn grid_point(i: u32) -> vec2<f32> { return vec2(f32(i % grid_w()) * 2.0 + 1.0, f32(i / grid_w()) * 2.0 + 1.0); }
/// Whether the simulation buffers belong to another scene, so this one
/// starts from scratch.
fn reset() -> bool { return mark(MARK_SIM) != params[H_SIM_ID]; }
fn at(base: u32, i: u32) -> u32 { return params[base] + i; }
fn sim_scene() -> u32 { return params[H_SIM_SCENE]; }
fn inside_sim(p: vec2<f32>) -> bool { return layer_coverage(select(0u, 1u, sim_scene() == H_TO), p.x, p.y) > 0.5; }
/// The ping-pong buffer this step reads, and the one it writes.
fn sim_src() -> u32 { return select(H_SIM0, H_SIM1, ((mark(MARK_PARITY) + FLIP) & 1u) == 1u); }
fn sim_dst() -> u32 { return select(H_SIM1, H_SIM0, ((mark(MARK_PARITY) + FLIP) & 1u) == 1u); }
/// A different number for every step of every frame.
fn step_seed() -> u32 { return frame() * 8u + STEP; }

/// Scene `s`'s ink colour at `(x, y)`.
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
fn fluid_advect(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    if reset() {
        state[at(H_SIM1, i)] = vec4(0.0);
        state[at(H_SIM3, i)] = vec4(0.0);
        return;
    }
    let p = grid_point(i);
    let t = time();
    let v = state[at(H_SIM0, i)].xy;
    let back = p - v * SIM_DT * 2.0;
    var velocity = grid_sample(H_SIM0, back).xy * 0.994;
    var dye = grid_sample(H_SIM2, back).rgb * 0.982;
    // Eddies: the curl of drifting noise, which swirls without pushing the
    // dye anywhere in particular.
    let q = p * 0.025 + vec2(t * 0.07, -t * 0.05);
    let e = 0.5;
    let dx = fbm(q + vec2(e, 0.0)) - fbm(q - vec2(e, 0.0));
    let dy = fbm(q + vec2(0.0, e)) - fbm(q - vec2(0.0, e));
    velocity += vec2(dy, -dx) * 240.0 * SIM_DT;
    if inside_sim(p) {
        dye = mix(dye, ink_at(sim_scene(), p.x, p.y, t) * 0.85, 0.1);
    }
    state[at(H_SIM1, i)] = vec4(velocity, 0.0, 0.0);
    state[at(H_SIM3, i)] = vec4(dye, 1.0);
}

/// The fluid's spin at each point, for vorticity confinement.
@compute @workgroup_size(64)
fn fluid_curl(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let n = neighbours(H_SIM1, gx, gy);
    state[at(H_SIM0, i)] = vec4(0.5 * ((n[1].y - n[0].y) - (n[3].x - n[2].x)), 0.0, 0.0, 0.0);
}

/// Vorticity confinement: pushes along the spin's gradient, putting back the
/// small eddies that the coarse grid smears away.
@compute @workgroup_size(64)
fn fluid_vorticity(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let n = neighbours(H_SIM0, gx, gy);
    let spin = state[at(H_SIM0, i)].x;
    let gradient = vec2(abs(n[1].x) - abs(n[0].x), abs(n[3].x) - abs(n[2].x));
    let towards = gradient / max(length(gradient), 1e-5);
    let force = vec2(towards.y, -towards.x) * spin * 6.0;
    state[at(H_SIM1, i)] = vec4(state[at(H_SIM1, i)].xy + force * SIM_DT, 0.0, 0.0);
}

@compute @workgroup_size(64)
fn fluid_divergence(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let n = neighbours(H_SIM1, gx, gy);
    let divergence = 0.5 * ((n[1].x - n[0].x) + (n[3].y - n[2].y));
    state[at(H_SIM0, i)] = vec4(0.0, 0.0, 0.0, divergence);
}

@compute @workgroup_size(64)
fn fluid_pressure(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
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
fn fluid_project(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let n = neighbours(H_SIM0, gx, gy);
    let gradient = 0.5 * vec2(n[1].z - n[0].z, n[3].z - n[2].z);
    let velocity = state[at(H_SIM1, i)].xy - gradient;
    state[at(H_SIM1, i)] = vec4(velocity, 0.0, 0.0);
}

@compute @workgroup_size(64)
fn fluid_copy(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    state[at(H_SIM0, i)] = vec4(state[at(H_SIM1, i)].xy, 0.0, 0.0);
    state[at(H_SIM2, i)] = state[at(H_SIM3, i)];
}

// ---------------------------------------------------------------- reaction-diffusion

@compute @workgroup_size(64)
fn reaction(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let src = sim_src();
    let dst = sim_dst();
    let p = grid_point(i);
    let inside = inside_sim(p);
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
    // A pattern grows inside the letters and dies away outside them: coral,
    // mitosis, worms, a maze or spots, by the scene's variant.
    let presets = array<vec2<f32>, 5>(vec2(0.0545, 0.062), vec2(0.0367, 0.0649), vec2(0.078, 0.061), vec2(0.029, 0.057),
        vec2(0.035, 0.065));
    let preset = presets[variant(sim_scene()) % 5u];
    let feed = select(0.02, preset.x, inside);
    let kill = select(0.08, preset.y, inside);
    let uvv = c.x * c.y * c.y;
    var u = c.x + (1.0 * lap.x - uvv + feed * (1.0 - c.x));
    var v = c.y + (0.5 * lap.y + uvv - (feed + kill) * c.y);
    if inside && hash3(i, frame(), 7u) < 0.0004 { v = 0.5; }
    state[at(dst, i)] = vec4(clamp(u, 0.0, 1.0), clamp(v, 0.0, 1.0), 0.0, 0.0);
}

// ---------------------------------------------------------------- life

@compute @workgroup_size(64)
fn life(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let src = sim_src();
    let dst = sim_dst();
    let p = grid_point(i);
    let inside = inside_sim(p);
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
    let chance = hash3(i, step_seed(), 11u);
    if (inside && chance < 0.03) || chance < 0.0002 { alive = 1.0; }
    let trail = select(here.y * 0.8, 1.0, alive > 0.5);
    state[at(dst, i)] = vec4(alive, trail, 0.0, 0.0);
}

// ---------------------------------------------------------------- fire

@compute @workgroup_size(64)
fn fire(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let src = sim_src();
    let dst = sim_dst();
    if reset() { state[at(dst, i)] = vec4(0.0); return; }
    let p = grid_point(i);
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let r = hash3(i, step_seed(), 3u);
    let drift = i32(floor(r * 3.0)) - 1;
    let below = grid_load(src, gx + drift, gy + 1);
    var heat: f32;
    if inside_sim(p) {
        heat = 0.8 + 0.2 * r;
    } else if gy == i32(grid_h()) - 1 {
        heat = 0.45 * r;
    } else {
        heat = below.x - (0.008 + r * 0.03);
    }
    // Smoke: what the flames lose rises on, slowly thinning.
    let smoke = below.y * 0.975 + max(below.x - max(heat, 0.0), 0.0) * 0.35;
    state[at(dst, i)] = vec4(max(heat, 0.0), clamp(smoke, 0.0, 1.0), 0.0, 0.0);
}

// ---------------------------------------------------------------- ripples

@compute @workgroup_size(64)
fn ripple(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let src = sim_src();
    let dst = sim_dst();
    if reset() { state[at(dst, i)] = vec4(0.0); return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let here = grid_load(src, gx, gy);
    let n = neighbours(src, gx, gy);
    let lap = n[0].x + n[1].x + n[2].x + n[3].x - 4.0 * here.x;
    var h = (2.0 * here.x - here.y + 0.25 * lap) * 0.996;
    // Rain: a few drops each second.
    let drop = hash3(u32(gx / 4), u32(gy / 4) * 7919u, step_seed());
    if drop < 0.000008 { h += 8.0; }
    state[at(dst, i)] = vec4(h, here.x, 0.0, 0.0);
}

// ---------------------------------------------------------------- display

/// The simulation's newest buffer, once this frame's steps have run.
fn sim_latest() -> u32 { return select(H_SIM0, H_SIM1, ((mark(MARK_PARITY) + params[H_SIM_STEPS]) & 1u) == 1u); }

fn fire_color(s: u32, heat: f32) -> vec3<f32> {
    let h = clamp(heat, 0.0, 1.0);
    let flame = mixl(mixl(mixl(hex(0x200000u), hex(0xd42000u), h * 2.2), hex(0xffb020u), (h - 0.45) * 2.5), WHITE, (h - 0.85) * 5.0);
    return mixl(flame, sample(s, h * 0.6), 0.2);
}

/// Scene `s` (as `layer`) drawn by its simulation at `(x, y)`, from the
/// simulation buffer `latest`.
fn sim_art(s: u32, layer: u32, x: f32, y: f32, t: f32, row: u32, latest: u32) -> Art {
    var out = Art(vec3(0.0), 0.0, vec3(0.0), 0.0, 0.0);
    let p = vec2(x, y);
    switch mode(s) {
        case M_FLUID: {
            let dye = grid_sample(H_SIM2, p).rgb;
            let strength = max(dye.r, max(dye.g, dye.b));
            out.alpha = clamp(strength * 1.6, 0.0, 1.0);
            out.color = dye / max(strength, 0.001) * min(strength * 1.3, 1.0);
        }
        case M_REACTION: {
            let v = grid_sample(latest, p).y;
            let ink = ink_at(s, x, y, t);
            out.alpha = smoothstep(0.12, 0.3, v);
            out.color = mixl(ink * 0.6, lerp(ink, WHITE, 0.3), smoothstep(0.2, 0.4, v));
            out.glow = ink * smoothstep(0.05, 0.2, v) * 0.15;
        }
        case M_LIFE: {
            let cell = grid_load(latest, i32(x / 2.0), i32(y / 2.0));
            let ink = ink_at(s, x, y, t);
            out.alpha = max(cell.x, cell.y * 0.55);
            out.color = select(ink * 0.7, lerp(ink, WHITE, 0.2), cell.x > 0.5);
        }
        case M_FIRE: {
            let cell = grid_sample(latest, p);
            let heat = cell.x;
            let smoke = clamp(cell.y * 1.3, 0.0, 0.8) * (1.0 - clamp(heat * 2.0, 0.0, 1.0));
            let a = art(s, layer, x, y, t, row);
            let flame = clamp(heat * 1.8, 0.0, 1.0);
            out.alpha = max(max(flame, smoke), a.alpha);
            let ember = mixl(background(s) * 0.5, a.color * 0.35, 0.6);
            let fumes = mixl(background(s) * 2.5, vec3(0.35), 0.5);
            out.color = select(select(fumes, fire_color(s, heat), flame >= smoke), ember, a.alpha > 0.5);
            out.glow = fire_color(s, heat) * heat * 0.25;
        }
        default: {}
    }
    return out;
}

/// The water's surface offset at `p` from the heights in buffer `base`, for
/// refracting everything below it.
fn ripple_offset(base: u32, p: vec2<f32>) -> vec2<f32> {
    let gx = i32(p.x / 2.0);
    let gy = i32(p.y / 2.0);
    let n = neighbours(base, gx, gy);
    return vec2(n[1].x - n[0].x, n[3].x - n[2].x);
}

/// Freezes the old scene's simulation as it last looked, when a new one is
/// about to take the buffers over: its colour and coverage, or for water
/// its heights.
@compute @workgroup_size(64)
fn snapshot(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() { return; }
    let held = mark(MARK_SIM);
    if held == 0u || held == params[H_SIM_ID] { return; }
    let latest = select(H_SIM0, H_SIM1, (mark(MARK_PARITY) & 1u) == 1u);
    if mode(H_FROM) == M_RIPPLE {
        state[at(H_SNAPSHOT, i)] = state[at(latest, i)];
        return;
    }
    let p = grid_point(i);
    let a = sim_art(H_FROM, 0u, p.x, p.y, time(), u32(p.y / 4.0), latest);
    state[at(H_SNAPSHOT, i)] = vec4(a.color * a.alpha, a.alpha);
}

/// The frozen old simulation at `(x, y)`.
fn frozen(x: f32, y: f32) -> Art {
    let v = grid_sample(H_SNAPSHOT, vec2(x, y));
    return Art(v.rgb / max(v.a, 1e-3), v.a, vec3(0.0), 0.0, 0.0);
}
