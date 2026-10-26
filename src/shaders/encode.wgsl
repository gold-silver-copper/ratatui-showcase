// The encode pass: each terminal cell's 32 samples of the final image become
// one character and two colours. The samples are split into two groups of
// colour (2-means); the split's shape picks the glyph.

const L_SMOOTH: u32 = 0u;
const L_PIXELS: u32 = 1u;
const L_BRAILLE: u32 = 2u;
const L_ASCII: u32 = 3u;
const L_LED: u32 = 4u;
const L_SHAPES: u32 = 5u;
const L_CODE: u32 = 6u;

/// The table entry (mask, code point) nearest `mask`, from `count` entries
/// at header slot `base`.
fn nearest(base: u32, count: u32, mask: u32) -> vec2<u32> {
    var best = vec2(0u, 0x20u);
    var fewest = 64u;
    for (var g = 0u; g < count; g++) {
        let o = params[base] + g * 2u;
        let m = inputs[o];
        let d = countOneBits(m ^ mask);
        if d < fewest { fewest = d; best = vec2(m, inputs[o + 1u]); }
    }
    return best;
}

/// Average colour in linear light.
fn average(sum: vec3<f32>, n: f32) -> vec3<f32> { return select(vec3(0.0), sqrt(sum / n), n > 0.0); }

fn pack(c: vec3<f32>, cell: u32, which: u32) -> u32 {
    var v = clamp(c, vec3(0.0), vec3(1.0)) * 255.0;
    // Temporal dithering: a different tiny offset each frame, so colours
    // between the representable levels average out.
    if params[H_DITHER] == 1u { v += (hash3(cell * 2u + which, frame(), 13u) - 0.5) * 2.0; }
    let b = vec3<u32>(clamp(v + 0.5, vec3(0.0), vec3(255.0))) & vec3(0xfeu);
    return (b.r << 16u) | (b.g << 8u) | b.b;
}

const RAIN: array<u32, 24> = array<u32, 24>(0xff71u, 0xff72u, 0xff73u, 0xff74u, 0xff75u, 0xff76u, 0xff77u, 0xff78u, 0xff79u,
    0xff7au, 0xff7bu, 0xff7cu, 0xff7du, 0xff7eu, 0xff7fu, 0xff80u, 0xff81u, 0xff82u, 0x30u, 0x31u, 0x5au, 0x3au, 0x2bu, 0x2au);

@compute @workgroup_size(64)
fn encode(@builtin(global_invocation_id) id: vec3<u32>) {
    let cols = params[H_COLS];
    let rows = params[H_ROWS];
    let cell = id.x;
    if cell == 0u { output[0] = cols; output[1] = rows; }
    if cell >= cols * rows { return; }
    let cx = cell % cols;
    let cy = cell / cols;

    var c: array<vec3<f32>, 32>;
    var mean = vec3(0.0);
    var top = vec3(0.0);
    var bottom = vec3(0.0);
    for (var i = 0u; i < SAMPLES; i++) {
        let v = state[params[H_FINAL] + img_index(cx * COLS + i % COLS, cy * ROWS + i / COLS)].rgb;
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
    let plain = contrast < 0.05 || contrast * contrast < spread * 6.0 || n_on == 0.0 || n_off == 0.0;

    var glyph = 0x2580u;
    var fg = pixels_top;
    var bg = pixels_bottom;
    let look = params[H_LOOK];
    switch look {
        case 0u, 5u: {
            if !plain {
                let table = select(H_IN_GLYPHS, H_IN_ASCII, look == L_SHAPES);
                let count = select(params[H_GLYPHS], params[H_ASCII_COUNT], look == L_SHAPES);
                let best = nearest(table, count, mask);
                if best.x != 0u && best.x != 0xffffffffu {
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
                }
            } else if look == L_SHAPES {
                glyph = 0x20u;
                bg = sqrt(lin_mean);
            }
        }
        case 1u: {}
        case 2u: {
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
        case 3u: {
            let ramp = array<u32, 10>(0x20u, 0x2eu, 0x3au, 0x2du, 0x3du, 0x2bu, 0x2au, 0x23u, 0x25u, 0x40u);
            let coverage = select(n_on / 32.0, clamp(luma(sqrt(lin_mean)) * 1.4, 0.0, 1.0), plain);
            glyph = ramp[min(u32(round(coverage * 9.0)), 9u)];
            fg = select(fg_group, sqrt(lin_mean) * 1.3, plain);
            bg = select(bg_group, sqrt(lin_mean) * 0.25, plain);
        }
        case 4u: {
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
    output[2u + cell * 3u] = glyph;
    output[3u + cell * 3u] = pack(fg, cell, 0u);
    output[4u + cell * 3u] = pack(bg, cell, 1u);
}
