// Distance fields by jump flooding, every frame, on the grid. Each grid
// point keeps the nearest edge point of the old scene's art (xy) and the new
// one's (zw); -1 means none found yet. Seeding writes FIELD0, and twelve
// jumps (2048 down to 1) ping-pong so the result lands back in FIELD0.

fn grid_centre(gx: u32, gy: u32) -> vec2<f32> { return vec2(f32(gx) * 2.0 + 1.0, f32(gy) * 2.0 + 1.0); }

/// An edge point: where the art's inside differs from a neighbour's.
fn edge(layer: u32, p: vec2<f32>) -> bool {
    let here = layer_coverage(layer, p.x, p.y) > 0.5;
    let right = layer_coverage(layer, p.x + 2.0, p.y) > 0.5;
    let down = layer_coverage(layer, p.x, p.y + 2.0) > 0.5;
    return here != right || here != down;
}

@compute @workgroup_size(64)
fn field_seed(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
    let p = grid_centre(i % grid_w(), i / grid_w());
    var seeds = vec4(-1.0);
    if kind() != K_STILL && edge(0u, p) { seeds = vec4(p, seeds.zw); }
    if edge(1u, p) { seeds = vec4(seeds.xy, p); }
    state[params[H_FIELD0] + i] = seeds;
}

@compute @workgroup_size(64)
fn field_jump(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if i >= grid_w() * grid_h() { return; }
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
            if other.x >= 0.0 && (best.x < 0.0 || distance(p, other.xy) < distance(p, best.xy)) {
                best = vec4(other.xy, best.zw);
            }
            if other.z >= 0.0 && (best.z < 0.0 || distance(p, other.zw) < distance(p, best.zw)) {
                best = vec4(best.xy, other.zw);
            }
        }
    }
    state[params[dst] + i] = best;
}
