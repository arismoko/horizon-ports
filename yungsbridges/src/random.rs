//! The decoration random after RngInitializerPlacement reseeds it.
//!
//! Decoration draws from a WorldgenRandom over a XoroshiroRandomSource, and the original's
//! placement modifier calls `setSeed` on it before the bridge places. A feature's random can't
//! be reseeded, so the bridge carries on with its own generator seeded the same way: Xoroshiro
//! 128++ seeded by RandomSupport.upgradeSeedTo128bit, drawing floats as `nextLong() >>> 40`.

/// RngInitializerPlacement's seed for position (x, z) from its two draws `a` and `b` (each
/// already `| 1`).
pub(crate) fn position_seed(x: i32, z: i32, a: i64, b: i64) -> i64 {
    let x = i64::from(x)
        .wrapping_mul(a)
        .wrapping_mul(951_873_395_712)
        .wrapping_add(12_132_586);
    let z = i64::from(z)
        .wrapping_mul(b)
        .wrapping_mul(132_899_567_841)
        .wrapping_add(9_789_717);
    x.wrapping_mul(z) ^ 313_281_234
}

pub(crate) struct Xoroshiro {
    lo: i64,
    hi: i64,
}

/// RandomSupport.mixStafford13.
fn mix(mut z: i64) -> i64 {
    z = (z ^ ((z as u64) >> 30) as i64).wrapping_mul(-4_658_895_280_553_007_687);
    z = (z ^ ((z as u64) >> 27) as i64).wrapping_mul(-7_723_592_293_110_705_685);
    z ^ ((z as u64) >> 31) as i64
}

impl Xoroshiro {
    /// XoroshiroRandomSource.setSeed.
    pub(crate) fn new(seed: i64) -> Self {
        let lo = seed ^ 7_640_891_576_956_012_809;
        let hi = lo.wrapping_add(-7_046_029_254_386_353_131);
        let (lo, hi) = (mix(lo), mix(hi));
        if lo == 0 && hi == 0 {
            Self {
                lo: -7_046_029_254_386_353_131,
                hi: 7_640_891_576_956_012_809,
            }
        } else {
            Self { lo, hi }
        }
    }

    /// Xoroshiro128PlusPlus.nextLong.
    pub(crate) fn next_long(&mut self) -> i64 {
        let (l, mut m) = (self.lo, self.hi);
        let n = l.wrapping_add(m).rotate_left(17).wrapping_add(l);
        m ^= l;
        self.lo = l.rotate_left(49) ^ m ^ (m << 21);
        self.hi = m.rotate_left(28);
        n
    }

    /// WorldgenRandom.nextFloat: `next(24) * 2^-24`, its bits from the top of `nextLong`.
    pub(crate) fn next_float(&mut self) -> f32 {
        ((self.next_long() as u64) >> 40) as f32 * 5.960_464_5e-8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_zero_matches_java() {
        // new XoroshiroRandomSource(0L).nextLong(), as Java gives it.
        let mut r = Xoroshiro::new(0);
        assert_eq!(r.next_long(), 3_038_984_756_725_240_190);
    }

    #[test]
    fn position_seed_wraps_like_java_longs() {
        assert_eq!(
            position_seed(5, -7, 123_456_789, -987_654_321),
            5_906_561_133_384_492_074
        );
    }
}
