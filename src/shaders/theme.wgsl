// Theme colours and the ink shaders that paint the art.

fn rgb3(i: u32) -> vec3<f32> { return vec3(pf(i), pf(i + 1u), pf(i + 2u)); }
fn background(s: u32) -> vec3<f32> { return rgb3(s + S_BACKGROUND); }
fn accent(s: u32) -> vec3<f32> { return rgb3(s + S_ACCENT); }

/// The theme's looping gradient at `t`.
fn sample(s: u32, t: f32) -> vec3<f32> {
    let n = max(params[s + S_STOPS], 1u);
    let p = wrap(t) * f32(n);
    let i = u32(p) % n;
    let a = rgb3(s + S_STOP0 + i * 3u);
    let b = rgb3(s + S_STOP0 + ((i + 1u) % n) * 3u);
    return mix(a, b, smoothstep(0.0, 1.0, fract(p)));
}

fn sweep(position: f32, t: f32) -> f32 {
    let phase = (t * 0.35 - floor(t * 0.35 / 2.0) * 2.0) * 1.6 - 0.3;
    return exp(-pow(position * 0.8 - phase, 2.0) * 60.0);
}

fn sparkle(u: f32, v: f32, t: f32, density: f32) -> f32 {
    let cell = (u32(max(u * density, 0.0)) * 7919u) ^ (u32(max(v * density * 0.5, 0.0)) * 104729u);
    let phase = f32(cell % 997u) / 997.0;
    let s = sin((t * 0.8 + phase * 13.0) * TAU);
    return select(0.0, (s - 0.985) / 0.015, s > 0.985);
}

/// The scene's ink colour at `(u, v)` within the art.
fn shade_ink(s: u32, u: f32, v: f32, t: f32, row: u32) -> vec3<f32> {
    let d = length(vec2(u, v) - 0.5);
    switch params[s + S_SHADER] {
        case SH_FLOW: { return sample(s, 0.6 * u + 0.4 * v - 0.08 * t); }
        case SH_PLASMA: {
            // Plasma: interfering waves in a space that itself flows (domain
            // warping by drifting noise), so the bands curl, pinch and
            // split like hot gas. The palette runs across the bands, dark in
            // the troughs and bright on the crests, and the bands' edges
            // burn as thin white filaments.
            let p = vec2(u * 2.4, v * 1.5);
            let drift = vec2(fbm(p * 1.4 + vec2(t * 0.13, -t * 0.08)), fbm(p * 1.4 + vec2(5.2 - t * 0.1, 1.3 + t * 0.06)));
            let q = p + (drift - 0.5) * 2.6;
            let w = sin(q.x * 5.0 + t * 0.9) + sin(q.y * 6.0 - t * 1.15) + sin((q.x - q.y) * 3.5 + t * 0.6)
                + sin(length(q - vec2(1.2 + 0.4 * sin(t * 0.3), 0.75)) * 8.0 - t * 1.7);
            let n = w * 0.25;
            let band = sample(s, n * 0.55 + drift.x * 0.8 + t * 0.025);
            let lift = 0.4 + 0.6 * smoothstep(-0.9, 0.7, n);
            let filament = 1.0 - smoothstep(0.0, 0.16, abs(fract(w * 0.7 + t * 0.15) - 0.5));
            return lerp(band * lift, WHITE, filament * 0.55 + smoothstep(0.75, 1.0, n) * 0.3);
        }
        case SH_PULSE: { return sample(s, d * 1.6 - t * 0.15); }
        case SH_SHEEN: { return lerp(sample(s, u * 0.5 + t * 0.03), WHITE, sweep(u + v * 0.6, t) * 0.8); }
        case SH_RAINBOW: { return hsl(u * 0.9 + v * 0.3 - t * 0.1, 0.85, 0.62); }
        case SH_SCANLINE: {
            let line = select(0.72, 1.0, row % 2u == 0u);
            let roll_at = t * 0.25 - floor(t * 0.25 / 1.4) * 1.4;
            let roll = exp(-pow((v - roll_at + 0.2) * 9.0, 2.0));
            let flicker = 0.97 + 0.03 * sin(t * 13.0 * TAU);
            return lerp(sample(s, v * 0.8 + t * 0.05) * line * flicker, WHITE, roll * 0.35);
        }
        case SH_CHROME: {
            let horizon = 0.55 + 0.03 * sin(u * 6.0 + t * 0.7);
            var chrome: vec3<f32>;
            if v < horizon {
                chrome = mixl(hex(0x1a2a6cu), lerp(sample(s, 0.55), WHITE, 0.55), v / horizon);
            } else {
                let ground = lerp(sample(s, 0.1), hex(0xffcf70u), 0.4);
                chrome = mixl(hex(0x2b1200u), ground, pow((v - horizon) / (1.0 - horizon), 0.7));
            }
            return lerp(chrome, WHITE, sweep(u + v * 0.3, t) * 0.9);
        }
        case SH_HOLO: {
            let angle = atan2(v - 0.5, u - 0.5) / TAU;
            let foil = hsl(angle * 2.0 + d * 1.5 - t * 0.12, 0.7, 0.72);
            return lerp(mixl(foil, sample(s, u + t * 0.05), 0.35), WHITE, sparkle(u, v, t, 40.0) * 0.9);
        }
        case SH_FIRE: {
            let flicker = sin(u * 23.0 + t * 7.0) * 0.5 + sin(u * 41.0 - t * 11.0) * 0.25;
            let heat = clamp(1.15 - v + flicker * 0.12 + sin(t * 3.0 + u * 9.0) * 0.05, 0.0, 1.0);
            let flame = mixl(mixl(mixl(hex(0x400000u), hex(0xff3000u), heat * 1.6), hex(0xffd23fu), (heat - 0.5) * 2.0),
                WHITE, (heat - 0.85) * 4.0);
            return mixl(flame, sample(s, heat * 0.5), 0.25);
        }
        case SH_STRIPES: {
            let w = wrap((u * 2.0 + v * 1.2) * 4.0 - t * 0.6);
            let band = floor(w * 2.0) / 2.0;
            return lerp(sample(s, band * 0.5 + t * 0.03), WHITE, select(0.0, 0.4, w < 0.06));
        }
        case SH_MARBLE: {
            // Marble: veins warped by noise.
            let q = vec2(u * 3.0, v * 1.5);
            let warp = fbm(q + vec2(t * 0.05, 0.0)) * 4.0;
            let vein = abs(sin((u * 4.0 + v * 2.0 + warp) * PI));
            return mixl(sample(s, warp * 0.2 + t * 0.02), WHITE * 0.95, pow(1.0 - vein, 6.0) * 0.8);
        }
        case SH_LAVA: {
            // Lava: a dark crust broken into plates that drift on the melt
            // (domain-warped noise), the melt glowing through the cracks
            // between them and welling up in pools, white-hot where it is
            // hottest.
            let q = vec2(u * 3.0, v * 1.6);
            let a = fbm(q + vec2(0.0, t * 0.1));
            let b = fbm(q * 1.3 + vec2(a * 2.2, t * 0.07 + 3.0));
            let crack = 1.0 - smoothstep(0.0, 0.11, abs(b - 0.5));
            let pool = smoothstep(0.58, 0.74, b);
            let heat = clamp(max(crack, pool) * (0.75 + 0.25 * sin(t * 1.3 + a * 9.0)), 0.0, 1.0);
            let crust = mixl(hex(0x5a2410u), sample(s, a * 0.6 + 0.1) * 0.7, 0.4) * (0.75 + 0.5 * a);
            let melt = mixl(mixl(hex(0xc81e00u), hex(0xff8a10u), heat), hex(0xffe9a0u), smoothstep(0.75, 1.0, heat));
            return mixl(crust, mixl(melt, sample(s, 0.5 + heat * 0.3), 0.2), heat);
        }
        case SH_GOLD: {
            // Gold leaf: metallic gradient with moving highlights.
            let base = mixl(hex(0x6b4400u), hex(0xffd66bu), 0.5 + 0.5 * sin(v * 9.0 + u * 2.0 - t * 1.5));
            return lerp(mixl(base, sample(s, 0.2), 0.15), WHITE, sweep(u * 1.2 + v * 0.4, t * 1.3) * 0.9);
        }
        default: {
            // Glitter: many sparkling flecks over the gradient.
            let base = sample(s, u * 0.7 + v * 0.3 + t * 0.04);
            let glint = sparkle(u, v, t * 3.0, 160.0) + sparkle(u + 0.37, v + 0.11, t * 2.3, 110.0);
            return lerp(base * 0.85, WHITE, glint);
        }
    }
}
