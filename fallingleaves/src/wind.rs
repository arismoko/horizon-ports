//! Smooth-noise wind simulation matching Falling Leaves' `Wind`, `SmoothNoise`, and
//! `TriangularDistribution`.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const TAU: f32 = std::f32::consts::TAU;

/// Small deterministic xorshift64* PRNG for wind noise (avoids external crate dependencies).
#[derive(Clone, Debug)]
pub struct WindRng {
    state: u64,
}

impl WindRng {
    pub const fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform float in `[0.0, 1.0)`.
    pub fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as u32) as f32 / ((1u32 << 24) as f32)
    }

    /// Uniform integer in `0..bound` (returns 0 if `bound == 0`).
    pub fn next_usize(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() as usize) % bound
    }
}

/// Triangular probability distribution on `[a, b]` with mode `c`.
#[derive(Clone, Copy, Debug)]
pub struct TriangularDistribution {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    f: f32,
}

impl TriangularDistribution {
    pub const fn new(a: f32, b: f32, c: f32) -> Self {
        let f = (c - a) / (b - a);
        Self { a, b, c, f }
    }

    pub fn sample(&self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        if u < self.f {
            self.a + (u * (self.b - self.a) * (self.c - self.a)).sqrt()
        } else {
            self.b - ((1.0 - u) * (self.b - self.a) * (self.b - self.c)).sqrt()
        }
    }
}

/// Smoothly interpolated 1D value noise stepped every `tick_interval` ticks.
#[derive(Clone, Debug)]
pub struct SmoothNoise {
    left_noise: f32,
    right_noise: f32,
    ticks: u32,
    t: f32,
    tick_interval: u32,
}

impl SmoothNoise {
    /// Smoothstep polynomial `t^2 * (3 - 2t)` on `[0, 1]`.
    pub fn smoothstep(t: f32) -> f32 {
        t * t * (3.0 - 2.0 * t)
    }

    pub fn new(tick_interval: u32, initial: f32, next_right: f32) -> Self {
        Self {
            left_noise: initial,
            right_noise: next_right,
            ticks: 0,
            t: 0.0,
            tick_interval: tick_interval.max(1),
        }
    }

    pub fn tick<F>(&mut self, mut next_noise: F)
    where
        F: FnMut(f32) -> f32,
    {
        self.ticks = self.ticks.saturating_add(1);
        if self.ticks >= self.tick_interval {
            self.ticks = 0;
            self.left_noise = self.right_noise;
            self.right_noise = next_noise(self.left_noise);
        }
        self.t = self.ticks as f32 / self.tick_interval as f32;
    }

    /// Linear interpolation between left and right samples.
    pub fn get_lerp(&self) -> f32 {
        self.left_noise + self.t * (self.right_noise - self.left_noise)
    }

    /// Smoothstep interpolation between left and right samples.
    pub fn get_noise(&self) -> f32 {
        self.left_noise + Self::smoothstep(self.t) * (self.right_noise - self.left_noise)
    }
}

/// Wind strength state, matching `Wind.State`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindState {
    Calm,
    Windy,
    Stormy,
}

impl WindState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Calm => "CALM",
            Self::Windy => "WINDY",
            Self::Stormy => "STORMY",
        }
    }

    pub const fn distribution(self) -> TriangularDistribution {
        match self {
            Self::Calm => TriangularDistribution::new(0.05, 0.2, 0.05),
            Self::Windy => TriangularDistribution::new(0.05, 0.7, 0.3),
            Self::Stormy => TriangularDistribution::new(0.05, 1.1, 0.6),
        }
    }

    const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Calm,
            1 => Self::Windy,
            _ => Self::Stormy,
        }
    }
}

/// Client wind simulator (blocks / tick).
#[derive(Clone, Debug)]
pub struct Wind {
    rng: WindRng,
    pub wind_x: f32,
    pub wind_z: f32,
    velocity_noise: SmoothNoise,
    direction_trend_noise: SmoothNoise,
    direction_noise: SmoothNoise,
    was_raining: bool,
    was_thundering: bool,
    pub state: WindState,
    original_state: Option<WindState>,
    pub state_duration: i32,
}

impl Wind {
    pub fn new(seed: u64) -> Self {
        let mut rng = WindRng::new(seed);
        let state = WindState::Calm;
        let v_right = state.distribution().sample(rng.next_f32());
        let trend_initial = rng.next_f32() * TAU;
        let trend_right = rng.next_f32() * TAU;
        let dir_right = (2.0 * rng.next_f32() - 1.0) * TAU / 8.0;

        Self {
            rng,
            wind_x: 0.0,
            wind_z: 0.0,
            velocity_noise: SmoothNoise::new(2 * 20, 0.0, v_right),
            direction_trend_noise: SmoothNoise::new(30 * 60 * 20, trend_initial, trend_right),
            direction_noise: SmoothNoise::new(10 * 20, 0.0, dir_right),
            was_raining: false,
            was_thundering: false,
            state,
            original_state: None,
            state_duration: 0,
        }
    }

    pub fn reset(&mut self, seed: u64) {
        *self = Self::new(seed);
    }

    fn tick_state(
        &mut self,
        dimension: &str,
        is_raining: bool,
        is_thundering: bool,
        wind_enabled: bool,
        windless_dimensions: &[String],
    ) {
        self.state_duration -= 1;

        let is_windless = !wind_enabled || windless_dimensions.iter().any(|d| d == dimension);
        if is_windless {
            if self.original_state.is_none() {
                self.original_state = Some(self.state);
            }
            self.state = WindState::Calm;
            return;
        }

        if let Some(prev) = self.original_state.take() {
            self.state = prev;
        }

        let weather_changed =
            self.was_raining != is_raining || self.was_thundering != is_thundering;
        if weather_changed || self.state_duration <= 0 {
            if is_thundering {
                self.state = WindState::Stormy;
            } else {
                let index = self.rng.next_usize(2);
                self.state = WindState::from_index(if is_raining { index + 1 } else { index });
            }
            // Change wind state every 6 minutes (7200 ticks).
            self.state_duration = 6 * 60 * 20;
        }

        self.was_raining = is_raining;
        self.was_thundering = is_thundering;
    }

    pub fn tick(
        &mut self,
        dimension: &str,
        is_raining: bool,
        is_thundering: bool,
        wind_enabled: bool,
        windless_dimensions: &[String],
    ) {
        self.tick_state(
            dimension,
            is_raining,
            is_thundering,
            wind_enabled,
            windless_dimensions,
        );

        let dist = self.state.distribution();
        let rng = &mut self.rng;
        self.velocity_noise.tick(|_| dist.sample(rng.next_f32()));
        self.direction_trend_noise.tick(|_| rng.next_f32() * TAU);
        self.direction_noise
            .tick(|_| (2.0 * rng.next_f32() - 1.0) * TAU / 8.0);

        let strength = self.velocity_noise.get_noise();
        let direction = self.direction_trend_noise.get_lerp() + self.direction_noise.get_noise();

        self.wind_x = strength * direction.cos();
        self.wind_z = strength * direction.sin();
    }
}

/// Lock-free snapshot of live wind & rain state read by `CustomParticle::tick`.
pub struct WindSnapshot {
    wind_x: AtomicU32,
    wind_z: AtomicU32,
    is_raining: AtomicBool,
}

impl Default for WindSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl WindSnapshot {
    pub const fn new() -> Self {
        Self {
            wind_x: AtomicU32::new(0),
            wind_z: AtomicU32::new(0),
            is_raining: AtomicBool::new(false),
        }
    }

    pub fn store(&self, wind_x: f32, wind_z: f32, is_raining: bool) {
        let wx = if wind_x.is_finite() { wind_x } else { 0.0 };
        let wz = if wind_z.is_finite() { wind_z } else { 0.0 };
        self.wind_x.store(wx.to_bits(), Ordering::Relaxed);
        self.wind_z.store(wz.to_bits(), Ordering::Relaxed);
        self.is_raining.store(is_raining, Ordering::Relaxed);
    }

    pub fn load(&self) -> (f32, f32, bool) {
        (
            f32::from_bits(self.wind_x.load(Ordering::Relaxed)),
            f32::from_bits(self.wind_z.load(Ordering::Relaxed)),
            self.is_raining.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothstep_endpoints_and_midpoint() {
        assert_eq!(SmoothNoise::smoothstep(0.0), 0.0);
        assert_eq!(SmoothNoise::smoothstep(0.5), 0.5);
        assert_eq!(SmoothNoise::smoothstep(1.0), 1.0);
    }

    #[test]
    fn triangular_distribution_bounds() {
        let dist = WindState::Windy.distribution();
        assert!((dist.sample(0.0) - 0.05).abs() < 1e-6);
        assert!((dist.sample(1.0) - 0.7).abs() < 1e-6);
        let mid = dist.sample(0.5);
        assert!((0.05..=0.7).contains(&mid));
    }

    #[test]
    fn windless_dimension_forces_calm_and_restores_state() {
        let mut wind = Wind::new(42);
        let windless = vec!["minecraft:the_nether".to_owned()];

        // Thundering in overworld -> Stormy
        wind.tick("minecraft:overworld", true, true, true, &windless);
        assert_eq!(wind.state, WindState::Stormy);

        // Entering nether overrides to Calm
        wind.tick("minecraft:the_nether", true, true, true, &windless);
        assert_eq!(wind.state, WindState::Calm);

        // Returning to overworld restores Stormy
        wind.tick("minecraft:overworld", true, true, true, &windless);
        assert_eq!(wind.state, WindState::Stormy);
    }

    #[test]
    fn wind_snapshot_round_trips() {
        let snap = WindSnapshot::new();
        snap.store(0.25, -0.5, true);
        let (wx, wz, rain) = snap.load();
        assert_eq!(wx, 0.25);
        assert_eq!(wz, -0.5);
        assert!(rain);
    }
}
