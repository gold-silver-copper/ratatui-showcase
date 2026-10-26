// Distance fields by jump flooding, on the grid. Each grid point keeps the
// nearest edge point of the old scene's art (xy) and the new one's (zw); -1
// means none found yet.
//
// Covering writes each grid point's coverage to FIELD1; seeding marks the
// edges in FIELD0, halfway between grid points whose coverage differs; the
// jumps (from the largest that fits the grid down to 1, an even number of
// them) ping-pong so the result lands back in FIELD0; finishing turns it
// into signed distances in FIELD1. While the art holds still the fields are
// kept from the frame that built them, and every pass returns at once.

fn grid_centre(gx: u32, gy: u32) -> vec2<f32> { return vec2(f32(gx) * 2.0 + 1.0, f32(gy) * 2.0 + 1.0); }

/// Whether the fields from an earlier frame still hold.
fn fields_kept() -> bool {
    let key = params[H_FIELD_KEY];
    return key != 0u && mark(MARK_FIELD) == key;
}

/// How much of scene `layer` covers the grid point at `p`: the mean of its
/// four samples, so a stroke one sample wide still counts.
fn grid_coverage(layer: u32, p: vec2<f32>) -> f32 {
    return (layer_coverage(layer, p.x - 0.5, p.y - 0.5) + layer_coverage(layer, p.x + 0.5, p.y - 0.5)
        + layer_coverage(layer, p.x - 0.5, p.y + 0.5) + layer_coverage(layer, p.x + 0.5, p.y + 0.5)) / 4.0;
}

fn inside(c: f32) -> bool { return c >= 0.5; }

@compute @workgroup_size(64)
fn field_cover(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() || fields_kept() { return; }
    let p = grid_centre(i % grid_w(), i / grid_w());
    let old = select(0.0, grid_coverage(0u, p), kind() != K_STILL);
    state[params[H_FIELD1] + i] = vec4(old, grid_coverage(1u, p), 0.0, 0.0);
}

@compute @workgroup_size(64)
fn field_seed(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() || fields_kept() { return; }
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let p = grid_centre(u32(gx), u32(gy));
    let here = grid_load(H_FIELD1, gx, gy).xy;
    var steps = array<vec2<i32>, 4>(vec2(1, 0), vec2(-1, 0), vec2(0, 1), vec2(0, -1));
    var seeds = vec4(-1.0);
    for (var k = 0; k < 4; k++) {
        let q = vec2(gx, gy) + steps[k];
        if q.x < 0 || q.y < 0 || q.x >= i32(grid_w()) || q.y >= i32(grid_h()) { continue; }
        let there = grid_load(H_FIELD1, q.x, q.y).xy;
        // The edge lies halfway to a neighbour on its other side.
        let edge = p + vec2<f32>(steps[k]);
        if seeds.x < 0.0 && inside(here.x) != inside(there.x) { seeds = vec4(edge, seeds.zw); }
        if seeds.z < 0.0 && inside(here.y) != inside(there.y) { seeds = vec4(seeds.xy, edge); }
    }
    state[params[H_FIELD0] + i] = seeds;
}

fn distance2(a: vec2<f32>, b: vec2<f32>) -> f32 { let d = a - b; return dot(d, d); }

@compute @workgroup_size(64)
fn field_jump(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() || fields_kept() { return; }
    let src = select(H_FIELD0, H_FIELD1, FLIP == 1u);
    let dst = select(H_FIELD1, H_FIELD0, FLIP == 1u);
    let gx = i32(i % grid_w());
    let gy = i32(i / grid_w());
    let p = grid_centre(u32(gx), u32(gy));
    var best = grid_load(src, gx, gy);
    let jump = i32(STEP);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let x = gx + dx * jump;
            let y = gy + dy * jump;
            if (dx == 0 && dy == 0) || x < 0 || y < 0 || x >= i32(grid_w()) || y >= i32(grid_h()) { continue; }
            let other = grid_load(src, x, y);
            if other.x >= 0.0 && (best.x < 0.0 || distance2(p, other.xy) < distance2(p, best.xy)) {
                best = vec4(other.xy, best.zw);
            }
            if other.z >= 0.0 && (best.z < 0.0 || distance2(p, other.zw) < distance2(p, best.zw)) {
                best = vec4(best.xy, other.zw);
            }
        }
    }
    state[params[dst] + i] = best;
}

/// Turns the nearest edge points into signed distances (old scene in x, new
/// in y), so every later lookup is one bilinear read.
@compute @workgroup_size(64)
fn field_finish(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let i = thread(id, groups);
    if i >= grid_w() * grid_h() || fields_kept() { return; }
    let p = grid_centre(i % grid_w(), i / grid_w());
    let seeds = state[params[H_FIELD0] + i];
    var d = vec2(FAR);
    if seeds.x >= 0.0 {
        d.x = select(1.0, -1.0, inside(grid_coverage(0u, p))) * distance(p, seeds.xy);
    }
    if seeds.z >= 0.0 {
        d.y = select(1.0, -1.0, inside(grid_coverage(1u, p))) * distance(p, seeds.zw);
    }
    state[params[H_FIELD1] + i] = vec4(d, 0.0, 0.0);
}
