// The encode pass: each terminal cell's 32 samples of the final image become
// one character and two colours. The samples are split into two groups of
// colour (2-means); the split's shape picks the glyph. Every byte written
// to the terminal costs time, so a cell that barely changed keeps what it
// had, and a cell of one colour is a space with only a background.
//
// Hysteresis: where two glyphs, or a glyph and plain colour, fit a cell
// almost equally well, noise would flip it between them frame to frame. A
// cell keeps the glyph it shows (Region::Shown) unless the new one is
// clearly better; an edge that moves is much better at once, so motion
// stays sharp.

/// How much of the 3 x 3 samples around sample `i` of a cell `mask` covers,
/// staying inside the cell.
fn blurred_bit(mask: u32, i: u32) -> f32 {
    let x = i32(i % COLS);
    let y = i32(i / COLS);
    var sum = 0.0;
    var n = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let qx = x + dx;
            let qy = y + dy;
            if qx < 0 || qy < 0 || qx >= i32(COLS) || qy >= i32(ROWS) { continue; }
            sum += f32((mask >> u32(qy * i32(COLS) + qx)) & 1u);
            n += 1.0;
        }
    }
    return sum / n;
}

/// The glyph that fits best: the eight nearest by bits, then the one whose
/// blurred shape is closest to the blurred picture, so a stroke off by one
/// sample still finds its glyph (after Xu, Zhang and Wong's
/// misalignment-tolerant matching, SIGGRAPH 2010). `soft` is how far each
/// sample lies from the background colour toward the foreground.
fn best_glyph(base: u32, count: u32, mask: u32, soft: ptr<function, array<f32, 32>>) -> vec2<u32> {
    var picks = array<vec2<u32>, 8>();
    var bits = array<u32, 8>(64u, 64u, 64u, 64u, 64u, 64u, 64u, 64u);
    for (var g = 0u; g < count; g++) {
        let o = params[base] + g * 2u;
        let d = countOneBits(inputs[o] ^ mask);
        if d >= bits[7] { continue; }
        // Insert in order.
        var k = 7;
        while k > 0 && bits[k - 1] > d {
            bits[k] = bits[k - 1];
            picks[k] = picks[k - 1];
            k--;
        }
        bits[k] = d;
        picks[k] = vec2(inputs[o], inputs[o + 1u]);
    }
    var blurred = array<f32, 32>();
    for (var i = 0u; i < SAMPLES; i++) {
        var sum = 0.0;
        var n = 0.0;
        let x = i32(i % COLS);
        let y = i32(i / COLS);
        for (var dy = -1; dy <= 1; dy++) {
            for (var dx = -1; dx <= 1; dx++) {
                let qx = x + dx;
            let qy = y + dy;
                if qx < 0 || qy < 0 || qx >= i32(COLS) || qy >= i32(ROWS) { continue; }
                sum += (*soft)[u32(qy * i32(COLS) + qx)];
                n += 1.0;
            }
        }
        blurred[i] = sum / n;
    }
    var best = picks[0];
    var least = 1e9;
    for (var k = 0; k < 8; k++) {
        if bits[k] == 64u { break; }
        let m = picks[k].x;
        var err = 0.0;
        for (var i = 0u; i < SAMPLES; i++) {
            let exact = f32((m >> i) & 1u) - (*soft)[i];
            let near = blurred_bit(m, i) - blurred[i];
            err += near * near + 0.35 * exact * exact;
        }
        if err < least { least = err; best = picks[k]; }
    }
    return best;
}

/// How much worse than the new choice the shown glyph may fit before the
/// cell changes: a share, and a floor for nearly flat cells.
const KEEP: f32 = 0.2;
const KEEP_FLOOR: f32 = 0.004;

/// How badly a glyph of shape `mask` draws the samples `c`: each side
/// painted with the average of the samples it covers, the squared error
/// left, summed.
fn misfit(mask: u32, c: ptr<function, array<vec3<f32>, 32>>) -> f32 {
    var on = vec3(0.0);
    var off = vec3(0.0);
    var a = 0.0;
    var b = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let v = (*c)[i];
        if ((mask >> i) & 1u) == 1u { on += v; a += 1.0; } else { off += v; b += 1.0; }
    }
    on = select(vec3(0.0), on / a, a > 0.0);
    off = select(vec3(0.0), off / b, b > 0.0);
    var err = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let d = (*c)[i] - select(off, on, ((mask >> i) & 1u) == 1u);
        err += dot(d, d);
    }
    return err;
}

/// Average colour in linear light.
fn average(sum: vec3<f32>, n: f32) -> vec3<f32> { return select(vec3(0.0), sqrt(sum / n), n > 0.0); }

/// Blue noise for `cell`'s colour `which`: a 64 x 64 void-and-cluster mask
/// (noise.rs), moved on by the golden ratio each frame, so the error is
/// fine-grained in space and spread evenly over time.
fn blue_noise(cell: u32, which: u32) -> f32 {
    let x = (cell % params[H_COLS] + which * 32u) % 64u;
    let y = (cell / params[H_COLS]) % 64u;
    return wrap(bitcast<f32>(inputs[params[H_IN_NOISE] + y * 64u + x]) + f32(frame() % 1024u) * 0.618034);
}

fn pack(c: vec3<f32>, cell: u32, which: u32) -> u32 {
    var v = clamp(c, vec3(0.0), vec3(1.0)) * 255.0;
    // Temporal dithering: a different tiny offset each frame, so colours
    // between the representable levels average out.
    if params[H_DITHER] == 1u { v += (blue_noise(cell, which) - 0.5) * 2.0; }
    let b = vec3<u32>(clamp(v + 0.5, vec3(0.0), vec3(255.0))) & vec3(0xfeu);
    return (b.r << 16u) | (b.g << 8u) | b.b;
}

const RAIN: array<u32, 24> = array<u32, 24>(0xff71u, 0xff72u, 0xff73u, 0xff74u, 0xff75u, 0xff76u, 0xff77u, 0xff78u, 0xff79u,
    0xff7au, 0xff7bu, 0xff7cu, 0xff7du, 0xff7eu, 0xff7fu, 0xff80u, 0xff81u, 0xff82u, 0x30u, 0x31u, 0x5au, 0x3au, 0x2bu, 0x2au);

@compute @workgroup_size(64)
fn encode(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let cols = params[H_COLS];
    let rows = params[H_ROWS];
    let cell = thread(id, groups);
    if cell == 0u { output[0] = cols; output[1] = rows; }
    if cell >= cols * rows { return; }
    let cx = cell % cols;
    let cy = cell / cols;

    var c: array<vec3<f32>, 32>;
    var mean = vec3(0.0);
    var top = vec3(0.0);
    var bottom = vec3(0.0);
    for (var i = 0u; i < SAMPLES; i++) {
        let v = unpack4x8unorm(frames[final_index(0u, vec2(f32(cx * COLS + i % COLS), f32(cy * ROWS + i / COLS)))]).rgb;
        c[i] = v;
        mean += v * v;
        if i < 16u { top += v * v; } else { bottom += v * v; }
    }
    let lin_mean = mean / 32.0;
    let pixels_top = sqrt(top / 16.0);
    let pixels_bottom = sqrt(bottom / 16.0);

    // Two colour groups: split by brightness, then refine once.
    let threshold = luma(sqrt(lin_mean));
    var bright = vec3(0.0);
    var dark = vec3(0.0);
    var nb = 0.0;
    var nd = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        if luma(c[i]) >= threshold { bright += c[i]; nb += 1.0; } else { dark += c[i]; nd += 1.0; }
    }
    bright = select(vec3(0.0), bright / nb, nb > 0.0);
    dark = select(vec3(0.0), dark / nd, nd > 0.0);
    var mask = 0u;
    var sum_on = vec3(0.0);
    var sum_off = vec3(0.0);
    var n_on = 0.0;
    var n_off = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let on = dot(c[i] - bright, c[i] - bright) < dot(c[i] - dark, c[i] - dark);
        if on { mask |= 1u << i; sum_on += c[i] * c[i]; n_on += 1.0; } else { sum_off += c[i] * c[i]; n_off += 1.0; }
    }
    let fg_group = average(sum_on, n_on);
    let bg_group = average(sum_off, n_off);
    let contrast = length(fg_group - bg_group);
    // A real edge separates its groups by much more than they vary inside;
    // noise, grain and gentle gradients don't, and stay plain colour.
    var spread = 0.0;
    for (var i = 0u; i < SAMPLES; i++) {
        let centre = select(bg_group, fg_group, ((mask >> i) & 1u) == 1u);
        spread += dot(c[i] - centre, c[i] - centre);
    }
    spread /= 32.0;
    // What the cell showed last frame, if in this look: a plain cell needs
    // a little more to become a glyph, and a glyph a little less to stay.
    let shown = state[params[H_SHOWN] + cell];
    let held = bitcast<u32>(shown.z) == params[H_LOOK] + 1u;
    let was_plain = held && bitcast<u32>(shown.x) == 0u;
    let was_shaped = held && bitcast<u32>(shown.x) != 0u;
    let edge = select(select(1.0, 1.25, was_plain), 0.8, was_shaped);
    let plain = contrast < 0.05 * edge || contrast * contrast < spread * 6.0 * edge || n_on == 0.0 || n_off == 0.0;

    var glyph = 0x2580u;
    // The shape of the block glyph the cell shows, or 0 for plain colour.
    var shape = 0u;
    var fg = pixels_top;
    var bg = pixels_bottom;
    let look = params[H_LOOK];
    switch look {
        case L_SMOOTH, L_SHAPES: {
            var best = vec2(0u, 0x20u);
            if !plain {
                let table = select(H_IN_GLYPHS, H_IN_ASCII, look == L_SHAPES);
                let count = select(params[H_GLYPHS], params[H_ASCII_COUNT], look == L_SHAPES);
                // Where each sample lies between the two colours.
                var soft = array<f32, 32>();
                let span = fg_group - bg_group;
                for (var i = 0u; i < SAMPLES; i++) {
                    soft[i] = clamp(dot(c[i] - bg_group, span) / max(dot(span, span), 1e-6), 0.0, 1.0);
                }
                best = best_glyph(table, count, mask, &soft);
                // Keep the shown glyph unless the new one is clearly better.
                let before = vec2(bitcast<u32>(shown.x), bitcast<u32>(shown.y));
                if was_shaped && before.x != best.x && before.x != 0xffffffffu {
                    let now = misfit(best.x, &c);
                    if misfit(before.x, &c) <= now * (1.0 + KEEP) + KEEP_FLOOR { best = before; }
                }
            }
            let shaped = best.x != 0u && best.x != 0xffffffffu;
            if shaped {
                shape = best.x;
                glyph = best.y;
                // Each side of the glyph: the average of the samples it covers.
                var on = vec3(0.0);
                var off = vec3(0.0);
                var a = 0.0;
                var b = 0.0;
                for (var i = 0u; i < SAMPLES; i++) {
                    if ((best.x >> i) & 1u) == 1u { on += c[i] * c[i]; a += 1.0; } else { off += c[i] * c[i]; b += 1.0; }
                }
                fg = average(on, a);
                bg = average(off, b);
            } else if look == L_SHAPES {
                // ASCII has no half blocks: what no character fits is a space.
                glyph = 0x20u;
                bg = sqrt(lin_mean);
            }
        }
        case L_PIXELS: {}
        case L_BRAILLE: {
            var bits = 0u;
            for (var k = 0u; k < 8u; k++) {
                let dc = k % 2u;
                let dr = k / 2u;
                let s0 = dr * 2u * COLS + dc * 2u;
                let lit = ((mask >> s0) & 1u) + ((mask >> (s0 + 1u)) & 1u) + ((mask >> (s0 + COLS)) & 1u) + ((mask >> (s0 + COLS + 1u)) & 1u);
                if !plain && lit >= 2u { bits |= 1u << select(6u + dc, dr + 3u * dc, dr < 3u); }
            }
            glyph = select(0x20u, 0x2800u + bits, bits != 0u);
            fg = lerp(fg_group, WHITE, 0.08);
            bg = select(bg_group, sqrt(lin_mean), bits == 0u);
        }
        case L_ASCII: {
            let ramp = array<u32, 10>(0x20u, 0x2eu, 0x3au, 0x2du, 0x3du, 0x2bu, 0x2au, 0x23u, 0x25u, 0x40u);
            let coverage = select(n_on / 32.0, clamp(luma(sqrt(lin_mean)) * 1.4, 0.0, 1.0), plain);
            glyph = ramp[min(u32(round(coverage * 9.0)), 9u)];
            fg = select(fg_group, sqrt(lin_mean) * 1.3, plain);
            bg = select(bg_group, sqrt(lin_mean) * 0.25, plain);
        }
        case L_LED: {
            let lum = luma(sqrt(lin_mean));
            bg = select(bg_group, sqrt(lin_mean), plain) * 0.6;
            if (!plain && n_on >= 10.0 && luma(fg_group) > 0.2) || lum > 0.35 {
                glyph = 0x25cfu;
                fg = lerp(select(fg_group, sqrt(lin_mean), plain), WHITE, 0.08);
            } else {
                glyph = 0xb7u;
                fg = bg * 2.2 + vec3(0.04);
            }
        }
        case L_CONTOUR: {
            // Marching squares on the cell's quarters: where brightness
            // crosses a band, a line runs between the quarters it divides.
            var q = array<f32, 4>(0.0, 0.0, 0.0, 0.0);
            for (var i = 0u; i < SAMPLES; i++) {
                let k = select(0u, 1u, i % COLS >= 2u) + select(0u, 2u, i / COLS >= 4u);
                q[k] += luma(c[i]) / 8.0;
            }
            let bands = 7.0;
            let top = floor(max(max(q[0], q[1]), max(q[2], q[3])) * bands);
            var case_ = 0u;
            for (var k = 0u; k < 4u; k++) {
                if floor(q[k] * bands) >= top { case_ |= 1u << k; }
            }
            // Bits: 1 top left, 2 top right, 4 bottom left, 8 bottom right.
            let lines = array<u32, 16>(0x20u, 0x256fu, 0x2570u, 0x2500u, 0x256eu, 0x2502u, 0x2571u, 0x256du,
                0x256du, 0x2572u, 0x2502u, 0x256eu, 0x2500u, 0x2570u, 0x256fu, 0x20u);
            glyph = lines[case_];
            let picture = sqrt(lin_mean);
            fg = lerp(picture * 1.6 + vec3(0.05), WHITE, 0.1);
            bg = picture * 0.18;
        }
        case L_HALFTONE: {
            let dots = array<u32, 5>(0x20u, 0xb7u, 0x2219u, 0x2022u, 0x25cfu);
            let picture = sqrt(lin_mean);
            let level = clamp(luma(picture) * 1.5, 0.0, 1.0);
            glyph = dots[min(u32(round(level * 5.0)), 4u)];
            fg = picture / max(max(picture.r, max(picture.g, picture.b)), 0.05) * 0.95;
            bg = picture * 0.12;
        }
        case L_TEXT: {
            // The README, left to right, a line per row, drifting slowly.
            let n = max(params[H_TEXT_LEN], 1u);
            let at = (cell + u32(time() * 2.0)) % n;
            glyph = inputs[params[H_IN_TEXT] + at];
            let picture = sqrt(lin_mean);
            fg = lerp(picture * 1.5, WHITE, 0.05);
            bg = picture * 0.1;
        }
        default: {
            // Falling code, lit by the picture underneath.
            let t = time();
            let seed = hash(cx, 77u);
            let speed = 6.0 + seed * 10.0;
            let head = wrap(seed * 13.0 + t * speed / f32(rows * 2u)) * f32(rows) * 2.0;
            let behind = head - f32(cy);
            let trail = select(0.15, exp(-behind / 9.0), behind >= 0.0);
            let pick = u32(hash3(cx, cy, u32(t * 6.0 + seed * 11.0)) * 24.0);
            glyph = RAIN[min(pick, 23u)];
            let picture = select(fg_group, sqrt(lin_mean), plain);
            fg = lerp(picture * (0.35 + 1.1 * trail), WHITE, select(0.0, 0.7, behind >= 0.0 && behind < 1.0));
            bg = select(bg_group, sqrt(lin_mean), plain) * 0.22;
        }
    }
    var front = pack(fg, cell, 0u);
    let back = pack(bg, cell, 1u);
    // Half blocks whose halves match are a space, and a space's foreground
    // is never seen: one colour, so neighbours share it and it isn't resent.
    if glyph == 0x2580u && front == back { glyph = 0x20u; }
    if glyph == 0x20u { front = 0u; }
    let o = 2u + cell * 3u;
    // While the terminal falls behind (H_CALM), colours may drift further
    // before the cell is sent again.
    let slack = 2 + 3 * i32(params[H_CALM]);
    if params[H_DITHER] == 0u && output[o] == glyph && near(output[o + 1u], front, slack) && near(output[o + 2u], back, slack) {
        return;
    }
    output[o] = glyph;
    output[o + 1u] = front;
    output[o + 2u] = back;
    state[params[H_SHOWN] + cell] = vec4(bitcast<f32>(shape), bitcast<f32>(glyph), bitcast<f32>(look + 1u), 0.0);
}

/// Whether two packed colours differ by at most `slack` steps in each
/// channel.
fn near(a: u32, b: u32, slack: i32) -> bool {
    let d = abs(vec3<i32>(unpack4x8unorm(a).xyz * 255.0) - vec3<i32>(unpack4x8unorm(b).xyz * 255.0));
    return all(d <= vec3(slack));
}

/// The frame's last pass: records what this frame did for the next (see
/// the marks in common.wgsl).
@compute @workgroup_size(64)
fn finish(@builtin(global_invocation_id) id: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    if thread(id, groups) != 0u { return; }
    let m = marks();
    let steps = params[H_SIM_STEPS];
    if steps > 0u {
        output[m + MARK_SIM] = params[H_SIM_ID];
        output[m + MARK_PARITY] += steps;
    }
    output[m + MARK_FRAME] += 1u;
    output[m + MARK_SPLAT] = params[H_SPLAT_ID];
    output[m + MARK_FIELD] = params[H_FIELD_KEY];
}
