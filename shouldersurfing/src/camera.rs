//! The camera offset pipeline and the numbers around it (`ShoulderSurfingCamera`, the offset
//! event handlers and `Perspective`-independent camera maths).

use crate::config::{DistanceAttributeMode, K, Values};
use crate::math::{V3, lerp};

/// Multiplier and modifier of one situation: `v = v * mul + add`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Adjust {
    pub mul: V3,
    pub add: V3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OffsetCfg {
    pub base: V3,
    pub min: V3,
    pub max: V3,
    pub unlimited: [bool; 3],
    pub passenger: Adjust,
    pub sprint: Adjust,
    pub aiming: Adjust,
    pub fall_flying: Adjust,
    pub climbing: Adjust,
    pub distance_mode: DistanceAttributeMode,
    pub center_looking_down_angle: f64,
}

impl OffsetCfg {
    pub fn from_values(v: &Values) -> Self {
        let adjust = |mx, my, mz, ax, ay, az| Adjust {
            mul: v.v3(mx, my, mz),
            add: v.v3(ax, ay, az),
        };
        Self {
            base: v.v3(K::OffsetX, K::OffsetY, K::OffsetZ),
            min: v.v3(K::MinOffsetX, K::MinOffsetY, K::MinOffsetZ),
            max: v.v3(K::MaxOffsetX, K::MaxOffsetY, K::MaxOffsetZ),
            unlimited: [v.b(K::UnlimitedX), v.b(K::UnlimitedY), v.b(K::UnlimitedZ)],
            passenger: adjust(
                K::PassengerMulX,
                K::PassengerMulY,
                K::PassengerMulZ,
                K::PassengerModX,
                K::PassengerModY,
                K::PassengerModZ,
            ),
            sprint: adjust(
                K::SprintMulX,
                K::SprintMulY,
                K::SprintMulZ,
                K::SprintModX,
                K::SprintModY,
                K::SprintModZ,
            ),
            aiming: adjust(
                K::AimingMulX,
                K::AimingMulY,
                K::AimingMulZ,
                K::AimingModX,
                K::AimingModY,
                K::AimingModZ,
            ),
            fall_flying: adjust(
                K::FallFlyingMulX,
                K::FallFlyingMulY,
                K::FallFlyingMulZ,
                K::FallFlyingModX,
                K::FallFlyingModY,
                K::FallFlyingModZ,
            ),
            climbing: adjust(
                K::ClimbingMulX,
                K::ClimbingMulY,
                K::ClimbingMulZ,
                K::ClimbingModX,
                K::ClimbingModY,
                K::ClimbingModZ,
            ),
            distance_mode: DistanceAttributeMode::from_index(v.c(K::DistanceAttributeMode)),
            center_looking_down_angle: v.f(K::CenterLookingDown),
        }
    }
}

/// What the player is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffsetCtx {
    pub passenger: bool,
    pub sprinting: bool,
    pub aiming: bool,
    pub fall_flying: bool,
    pub climbing: bool,
    pub pitch: f32,
    /// `minecraft:scale` of the camera entity.
    pub scale: f64,
    /// `minecraft:camera_distance` of the camera entity, 4 by default.
    pub distance_attribute: f64,
}

impl Default for OffsetCtx {
    fn default() -> Self {
        Self {
            passenger: false,
            sprinting: false,
            aiming: false,
            fall_flying: false,
            climbing: false,
            pitch: 0.0,
            scale: 1.0,
            distance_attribute: VANILLA_CAMERA_DISTANCE,
        }
    }
}

/// The default value of the `minecraft:camera_distance` attribute.
pub const VANILLA_CAMERA_DISTANCE: f64 = 4.0;

fn apply(v: V3, a: &Adjust) -> V3 {
    [
        v[0] * a.mul[0] + a.add[0],
        v[1] * a.mul[1] + a.add[1],
        v[2] * a.mul[2] + a.add[2],
    ]
}

/// The offset the camera moves towards, in the order of the original's handler priorities:
/// distance attribute, passenger, sprinting, aiming, fall flying, climbing, centring when
/// looking down, entity scale and the limits.
pub fn target_offset(cfg: &OffsetCfg, ctx: &OffsetCtx) -> V3 {
    let mut v = cfg.base;
    match cfg.distance_mode {
        DistanceAttributeMode::Relative if VANILLA_CAMERA_DISTANCE > 0.0 => {
            v[2] *= ctx.distance_attribute / VANILLA_CAMERA_DISTANCE;
        }
        DistanceAttributeMode::Absolute => v[2] = ctx.distance_attribute,
        _ => {}
    }
    for (on, adjust) in [
        (ctx.passenger, &cfg.passenger),
        (ctx.sprinting, &cfg.sprint),
        (ctx.aiming, &cfg.aiming),
        (ctx.fall_flying, &cfg.fall_flying),
        (ctx.climbing, &cfg.climbing),
    ] {
        if on {
            v = apply(v, adjust);
        }
    }
    if cfg.center_looking_down_angle > 0.0
        && f64::from(ctx.pitch) >= 90.0 - cfg.center_looking_down_angle
    {
        v[0] = 0.0;
    }
    let scale = if ctx.scale.is_finite() && ctx.scale > 0.0 {
        ctx.scale
    } else {
        1.0
    };
    for i in 0..3 {
        v[i] *= scale;
        if !cfg.unlimited[i] {
            let (lo, hi) = (cfg.min[i].min(cfg.max[i]), cfg.min[i].max(cfg.max[i]));
            v[i] = v[i].clamp(lo * scale, hi * scale);
        }
    }
    v
}

/// The camera's current offset and the one a tick before, for interpolation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offsets {
    pub prev: V3,
    pub cur: V3,
}

impl Offsets {
    pub fn new(at: V3) -> Self {
        Self { prev: at, cur: at }
    }

    /// One client tick towards `target` at `speed` (0..1) of the way.
    pub fn tick(&mut self, target: V3, speed: f64) {
        self.prev = self.cur;
        let s = speed.clamp(0.0, 1.0);
        for i in 0..3 {
            self.cur[i] = lerp(s, self.cur[i], target[i]);
        }
    }

    pub fn render(&self, partial: f64) -> V3 {
        let t = partial.clamp(0.0, 1.0);
        [
            lerp(t, self.prev[0], self.cur[0]),
            lerp(t, self.prev[1], self.cur[1]),
            lerp(t, self.prev[2], self.cur[2]),
        ]
    }
}

/// The 8 corners the camera box is traced with (`ShoulderSurfingCamera.maxZoom`).
pub const CORNERS: [[f64; 3]; 8] = {
    const D: f64 = 0.1;
    [
        [D, D, D],
        [D, D, -D],
        [D, -D, D],
        [D, -D, -D],
        [-D, D, D],
        [-D, D, -D],
        [-D, -D, D],
        [-D, -D, -D],
    ]
};

/// How far (0..1) along the offset the camera may go, given the fraction each corner trace
/// reached (`None` for a trace that hit nothing). A tiny gap stays free of the wall.
pub fn zoom_fraction(hits: &[Option<f64>]) -> f64 {
    hits.iter()
        .flatten()
        .fold(1.0_f64, |acc, f| acc.min(f.clamp(0.0, 1.0)))
}

/// Steps one axis of the offset by the key press: left, up and out are `+step` (`dir = 1`).
pub fn adjust_axis(value: f64, dir: f64, step: f64, min: f64, max: f64, unlimited: bool) -> f64 {
    let moved = value + dir * step;
    if unlimited {
        moved
    } else {
        moved.clamp(min.min(max), min.max(max))
    }
}

/// `Presets.toggle`: the preset after the closest one, wrapping to the first. None for no
/// presets.
pub fn next_preset(presets: &[f64], current: f64) -> Option<f64> {
    let (closest, _) = presets
        .iter()
        .enumerate()
        .filter(|(_, p)| p.is_finite())
        .min_by(|(_, a), (_, b)| (*a - current).abs().total_cmp(&(*b - current).abs()))?;
    presets.get((closest + 1) % presets.len()).copied()
}

/// The size of the box a body is traced as: for `isInsideEntity`.
pub fn inside_head(camera_distance: f64, bb_width: f64, keep_out_multiplier: f64) -> bool {
    camera_distance < bb_width * keep_out_multiplier
}

/// `isLookingUp`: the camera looks above the hide angle (pitch counts down when looking up).
pub fn looking_up(pitch: f32, hide_angle: f64) -> bool {
    f64::from(pitch) < hide_angle - 90.0
}

const MIN_PLAYER_ALPHA: f64 = 0.15;

fn ease_out_circ(x: f64) -> f64 {
    (1.0 - (x - 1.0).powi(2)).max(0.0).sqrt()
}

/// `ComputeCameraEntityTransparencyEventHandlerImpl`: the player's alpha (0.15..1) from how far
/// the camera has moved off the eye.
pub fn player_alpha(
    render: V3,
    configured: V3,
    bb_width: f64,
    bb_height: f64,
    eye_height: f64,
) -> f64 {
    let ratio = |value: f64, limit: f64| {
        if limit == 0.0 {
            1.0
        } else {
            (value / limit).clamp(0.0, 1.0)
        }
    };
    let alpha_x = ratio(render[0].abs(), configured[0].abs().min(bb_width / 2.0));
    let alpha_y = if render[1] > 0.0 {
        ratio(render[1], configured[1].min(bb_height - eye_height))
    } else if render[1] < 0.0 {
        ratio(render[1], configured[1].min(eye_height))
    } else {
        0.0
    };
    let alpha_xy = if alpha_x < 1.0 || alpha_y < 1.0 {
        (alpha_x * alpha_x + alpha_y * alpha_y).sqrt()
    } else {
        1.0
    };
    let limit_z = bb_height.min(configured[2]);
    let alpha_z = 1.0
        - ease_out_circ(if limit_z == 0.0 {
            0.0
        } else {
            (1.0 - render[2].abs() / limit_z).clamp(0.0, 1.0)
        });
    alpha_xy.min(alpha_z).clamp(MIN_PLAYER_ALPHA, 1.0)
}

/// Whether the player is drawn at all: the closest the alpha gets without a transparency
/// pipeline is "fully hidden at the floor".
pub fn player_hidden(alpha: f64) -> bool {
    alpha <= MIN_PLAYER_ALPHA
}

/// The camera's decoupled rotation and the free-look offset on it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rotation {
    pub yaw: f32,
    pub pitch: f32,
}

impl Rotation {
    /// Adds a turn, clamping the pitch like `Entity.turn`.
    pub fn turn(&mut self, d_yaw: f32, d_pitch: f32) {
        if d_yaw.is_finite() {
            self.yaw = crate::math::wrap_degrees(self.yaw + d_yaw);
        }
        if d_pitch.is_finite() {
            self.pitch = (self.pitch + d_pitch).clamp(-90.0, 90.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OffsetCfg {
        OffsetCfg::from_values(&Values::default())
    }

    #[test]
    fn defaults_give_the_original_offset() {
        assert_eq!(
            target_offset(&cfg(), &OffsetCtx::default()),
            [-0.75, 0.0, 4.0]
        );
    }

    #[test]
    fn distance_attribute_modes() {
        let mut c = cfg();
        let ctx = OffsetCtx {
            distance_attribute: 8.0,
            ..OffsetCtx::default()
        };
        assert_eq!(target_offset(&c, &ctx)[2], 5.0); // 4 * 2, limited to the maximum 5
        c.distance_mode = DistanceAttributeMode::Absolute;
        c.unlimited = [true; 3];
        assert_eq!(target_offset(&c, &ctx)[2], 8.0);
        c.distance_mode = DistanceAttributeMode::Ignore;
        assert_eq!(target_offset(&c, &ctx)[2], 4.0);
    }

    #[test]
    fn situations_apply_in_order_and_climbing_centres() {
        let mut c = cfg();
        c.sprint = Adjust {
            mul: [1.0, 1.0, 0.5],
            add: [0.0, 0.0, 0.0],
        };
        c.aiming = Adjust {
            mul: [1.0, 1.0, 1.0],
            add: [0.0, 0.0, 1.0],
        };
        let ctx = OffsetCtx {
            sprinting: true,
            aiming: true,
            ..OffsetCtx::default()
        };
        assert_eq!(target_offset(&c, &ctx)[2], 3.0);
        let climbing = OffsetCtx {
            climbing: true,
            ..OffsetCtx::default()
        };
        assert_eq!(target_offset(&cfg(), &climbing)[0], 0.0);
    }

    #[test]
    fn looking_down_centres_x() {
        let ctx = OffsetCtx {
            pitch: 89.5,
            ..OffsetCtx::default()
        };
        assert_eq!(target_offset(&cfg(), &ctx)[0], 0.0);
        let ctx = OffsetCtx {
            pitch: 60.0,
            ..OffsetCtx::default()
        };
        assert_eq!(target_offset(&cfg(), &ctx)[0], -0.75);
    }

    #[test]
    fn scale_and_limits() {
        let ctx = OffsetCtx {
            scale: 2.0,
            ..OffsetCtx::default()
        };
        assert_eq!(target_offset(&cfg(), &ctx), [-1.5, 0.0, 8.0]);
        let bad = OffsetCtx {
            scale: f64::NAN,
            ..OffsetCtx::default()
        };
        assert_eq!(target_offset(&cfg(), &bad), [-0.75, 0.0, 4.0]);
        let mut c = cfg();
        c.base = [10.0, 10.0, 10.0];
        assert_eq!(target_offset(&c, &OffsetCtx::default()), [3.0, 1.5, 5.0]);
        c.unlimited = [true; 3];
        assert_eq!(target_offset(&c, &OffsetCtx::default()), [10.0, 10.0, 10.0]);
    }

    #[test]
    fn offsets_ease_and_interpolate() {
        let mut o = Offsets::new([0.0; 3]);
        o.tick([4.0, 0.0, 0.0], 0.25);
        assert_eq!(o.cur[0], 1.0);
        assert_eq!(o.render(0.5)[0], 0.5);
        o.tick([4.0, 0.0, 0.0], 5.0);
        assert_eq!(o.cur[0], 4.0);
    }

    #[test]
    fn zoom_takes_the_nearest_wall() {
        assert_eq!(zoom_fraction(&[None, None]), 1.0);
        assert_eq!(zoom_fraction(&[Some(0.6), None, Some(0.3)]), 0.3);
        assert_eq!(zoom_fraction(&[Some(-1.0)]), 0.0);
        assert_eq!(CORNERS.len(), 8);
    }

    #[test]
    fn key_steps_respect_limits() {
        assert_eq!(adjust_axis(0.0, 1.0, 0.025, -3.0, 3.0, false), 0.025);
        assert_eq!(adjust_axis(3.0, 1.0, 0.5, -3.0, 3.0, false), 3.0);
        assert_eq!(adjust_axis(3.0, 1.0, 0.5, -3.0, 3.0, true), 3.5);
        assert_eq!(adjust_axis(0.0, -1.0, 1.0, 5.0, -5.0, false), -1.0);
    }

    #[test]
    fn presets_wrap() {
        assert_eq!(next_preset(&[], 0.0), None);
        assert_eq!(next_preset(&[-0.75, 0.75], -0.7), Some(0.75));
        assert_eq!(next_preset(&[-0.75, 0.75], 0.8), Some(-0.75));
        assert_eq!(next_preset(&[1.0], 5.0), Some(1.0));
    }

    #[test]
    fn head_and_looking_up() {
        assert!(inside_head(0.3, 0.6, 0.75));
        assert!(!inside_head(0.5, 0.6, 0.75));
        assert!(looking_up(-80.0, 20.0));
        assert!(!looking_up(-60.0, 20.0));
        assert!(!looking_up(-89.0, 0.0));
    }

    #[test]
    fn alpha_floor_and_full() {
        // Camera on the eye: fully faded to the floor.
        let a = player_alpha([0.0, 0.0, 0.0], [-0.75, 0.0, 4.0], 0.6, 1.8, 1.62);
        assert_eq!(a, MIN_PLAYER_ALPHA);
        assert!(player_hidden(a));
        // Far behind and to the side: opaque.
        let a = player_alpha([-0.75, 0.0, 4.0], [-0.75, 0.0, 4.0], 0.6, 1.8, 1.62);
        assert_eq!(a, 1.0);
        assert!(!player_hidden(a));
        // A zero configured offset never divides by zero.
        let a = player_alpha([0.0; 3], [0.0; 3], 0.6, 1.8, 1.62);
        assert!(a.is_finite());
    }

    #[test]
    fn rotation_turns_and_clamps() {
        let mut r = Rotation::default();
        r.turn(190.0, 100.0);
        assert_eq!(
            r,
            Rotation {
                yaw: -170.0,
                pitch: 90.0
            }
        );
        r.turn(f32::NAN, f32::INFINITY);
        assert_eq!(
            r,
            Rotation {
                yaw: -170.0,
                pitch: 90.0
            }
        );
    }
}
