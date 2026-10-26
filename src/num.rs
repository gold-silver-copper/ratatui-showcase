//! Conversions between floats and integers without `as` casts, which the
//! lints forbid because they truncate silently. Each one saturates instead.

/// `v` rounded down, clamped to `0..=u32::MAX`; NaN gives 0.
pub fn floor_u32(v: f32) -> u32 {
    if v.is_nan() || v < 1.0 {
        return 0;
    }
    if v >= 4_294_967_040.0 {
        return u32::MAX;
    }
    // An f32 is 1.mantissa x 2^(exponent - 127); for v >= 1 the exponent is
    // at least 127, so the integer part is the mantissa shifted into place.
    let bits = v.to_bits();
    let exponent = (bits >> 23) & 0xff;
    let mantissa = (bits & 0x7f_ffff) | 0x80_0000;
    let shift = exponent.saturating_sub(127);
    if shift >= 23 {
        mantissa
            .checked_shl(shift.saturating_sub(23))
            .unwrap_or(u32::MAX)
    } else {
        mantissa
            .checked_shr(23_u32.saturating_sub(shift))
            .unwrap_or(0)
    }
}

/// `v` rounded down to an index, clamped at 0.
pub fn floor_usize(v: f32) -> usize {
    usize::try_from(floor_u32(v)).unwrap_or(usize::MAX)
}

/// `v` rounded down, clamped to `0..=u16::MAX`.
pub fn floor_u16(v: f32) -> u16 {
    u16::try_from(floor_u32(v)).unwrap_or(u16::MAX)
}

/// `v` rounded to the nearest byte, clamped to `0..=255`.
pub fn round_u8(v: f32) -> u8 {
    u8::try_from(floor_u32(v + 0.5)).unwrap_or(u8::MAX)
}

/// `v` as an `f32`, its mantissa cut to fit; tiny values become 0 and huge
/// ones infinity.
pub fn narrow(v: f64) -> f32 {
    let bits = v.to_bits();
    let sign = u32::from(bits >> 63 == 1) << 31;
    let exponent = i64::try_from((bits >> 52) & 0x7ff)
        .unwrap_or(0)
        .saturating_sub(1023);
    if v == 0.0 || exponent < -126 {
        return f32::from_bits(sign);
    }
    if exponent > 127 || !v.is_finite() {
        return f32::from_bits(sign | 0x7f80_0000);
    }
    let mantissa = u32::try_from((bits >> 29) & 0x7f_ffff).unwrap_or(0);
    let biased = u32::try_from(exponent.saturating_add(127)).unwrap_or(0);
    f32::from_bits(sign | biased << 23 | mantissa)
}

/// `n` as an `f32`, rounding to the nearest representable value.
pub fn float(n: usize) -> f32 {
    let n = u32::try_from(n).unwrap_or(u32::MAX);
    // Exact below 2^24; above that, the nearest f32.
    let high = u16::try_from(n >> 16).unwrap_or(u16::MAX);
    let low = u16::try_from(n & 0xffff).unwrap_or(u16::MAX);
    f32::from(high) * 65_536.0 + f32::from(low)
}

/// `n` as a `u16`, clamped.
pub fn u16_of(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// `n` as a `u32`, clamped.
pub fn u32_of(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floors_match_truncation() {
        for (v, floor) in [
            (0.0, 0),
            (0.5, 0),
            (1.0, 1),
            (1.5, 1),
            (2.999, 2),
            (3.0, 3),
            (255.4, 255),
            (1023.9, 1023),
            (65_536.5, 65_536),
            (1.0e7, 10_000_000),
        ] {
            assert_eq!(floor_u32(v), floor, "{v}");
        }
    }

    #[test]
    fn floors_saturate() {
        assert_eq!(floor_u32(-3.0), 0);
        assert_eq!(floor_u32(f32::NAN), 0);
        assert_eq!(floor_u32(f32::INFINITY), u32::MAX);
        assert_eq!(round_u8(300.0), 255);
        assert_eq!(round_u8(127.5), 128);
    }

    #[test]
    fn narrowing_keeps_values() {
        for v in [
            1.0,
            -0.5,
            0.131_825_904_205_311_97,
            -0.743_643_887_037_158_7,
            3.0e-20,
            0.0,
        ] {
            assert!((f64::from(narrow(v)) - v).abs() <= v.abs() * 1e-6, "{v}");
        }
        assert_eq!(narrow(1e300), f32::INFINITY);
    }

    #[test]
    fn floats_round_trip() {
        for n in [0, 1, 4, 255, 65_535, 65_536, 1 << 20] {
            assert_eq!(floor_usize(float(n)), n);
        }
    }
}
