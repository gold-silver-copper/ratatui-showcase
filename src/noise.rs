//! A blue-noise mask: 64 x 64 thresholds whose ranks are spread as evenly as
//! possible, by Ulichney's void-and-cluster method. Dithering with it leaves
//! its error at high frequencies, which the eye averages away; offsetting it
//! by the golden ratio each frame spreads the error evenly over time too.

const SIZE: usize = 64;
const SIGMA: f32 = 1.9;

/// Each texel's rank in `0.0..1.0`, row by row.
pub fn mask() -> Vec<f32> {
    let n = SIZE.saturating_mul(SIZE);
    // How much each texel crowds every other, on a torus.
    let weight: Vec<f32> = (0..n)
        .map(|i| {
            let wrap = |d: usize| d.min(SIZE.saturating_sub(d));
            let (dx, dy) = (wrap(i % SIZE), wrap(i / SIZE));
            let r2 = crate::num::float(dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy)));
            (-r2 / (2.0 * SIGMA * SIGMA)).exp()
        })
        .collect();
    let crowd = |at: usize, other: usize| {
        // Torus offsets: SIZE is a power of two, so wrapping is a mask.
        let dx = (other % SIZE).wrapping_sub(at % SIZE) & (SIZE - 1);
        let dy = (other / SIZE).wrapping_sub(at / SIZE) & (SIZE - 1);
        weight
            .get(dy.wrapping_mul(SIZE) | dx)
            .copied()
            .unwrap_or(0.0)
    };
    let mut on = vec![false; n];
    let mut energy = vec![0.0_f32; n];
    let toggle = |on: &mut Vec<bool>, energy: &mut Vec<f32>, at: usize, set: bool| {
        if let Some(slot) = on.get_mut(at) {
            *slot = set;
        }
        let sign = if set { 1.0 } else { -1.0 };
        for (i, e) in energy.iter_mut().enumerate() {
            *e += sign * crowd(at, i);
        }
    };
    // Start from a tenth of the texels at random, then move the tightest
    // cluster into the largest void until that stops changing anything.
    let mut rng = fastrand::Rng::with_seed(1);
    let first = n / 10;
    let mut placed = 0;
    while placed < first {
        let at = rng.usize(..n);
        if on.get(at) == Some(&false) {
            toggle(&mut on, &mut energy, at, true);
            placed = placed.saturating_add(1);
        }
    }
    let tightest = |on: &[bool], energy: &[f32]| {
        (0..n)
            .filter(|&i| on.get(i) == Some(&true))
            .max_by(|&a, &b| {
                energy
                    .get(a)
                    .unwrap_or(&0.0)
                    .total_cmp(energy.get(b).unwrap_or(&0.0))
            })
    };
    let emptiest = |on: &[bool], energy: &[f32]| {
        (0..n)
            .filter(|&i| on.get(i) == Some(&false))
            .min_by(|&a, &b| {
                energy
                    .get(a)
                    .unwrap_or(&0.0)
                    .total_cmp(energy.get(b).unwrap_or(&0.0))
            })
    };
    for _ in 0..n {
        let (Some(from), Some(to)) = (tightest(&on, &energy), emptiest(&on, &energy)) else {
            break;
        };
        toggle(&mut on, &mut energy, from, false);
        if emptiest(&on, &energy) == Some(from) {
            toggle(&mut on, &mut energy, from, true);
            break;
        }
        toggle(&mut on, &mut energy, to, true);
    }
    let mut rank = vec![0_usize; n];
    // Ranks below the start: take the tightest clusters away one by one.
    let (mut down_on, mut down_energy) = (on.clone(), energy.clone());
    for r in (0..first).rev() {
        let Some(at) = tightest(&down_on, &down_energy) else {
            break;
        };
        toggle(&mut down_on, &mut down_energy, at, false);
        if let Some(slot) = rank.get_mut(at) {
            *slot = r;
        }
    }
    // Ranks above: fill the largest voids one by one.
    for r in first..n {
        let Some(at) = emptiest(&on, &energy) else {
            break;
        };
        toggle(&mut on, &mut energy, at, true);
        if let Some(slot) = rank.get_mut(at) {
            *slot = r;
        }
    }
    rank.into_iter()
        .map(|r| (crate::num::float(r) + 0.5) / crate::num::float(n))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_rank_once_and_neighbours_differ() {
        let mask = super::mask();
        let mut sorted = mask.clone();
        sorted.sort_by(f32::total_cmp);
        sorted.dedup();
        assert_eq!(sorted.len(), 64 * 64);
        // Blue noise: horizontal neighbours are anticorrelated, unlike
        // white noise.
        let pairs = mask.iter().zip(mask.iter().skip(1));
        let mean_gap = pairs.map(|(a, b)| (a - b).abs()).sum::<f32>() / (64.0 * 64.0 - 1.0);
        assert!(mean_gap > 0.36, "{mean_gap}");
    }
}
