//! The four perspectives and how F5 cycles through them (`Perspective.java`).

use mod_api::CameraType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perspective {
    FirstPerson,
    ThirdPersonBack,
    ThirdPersonFront,
    ShoulderSurfing,
}

/// Which perspectives the player allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Enabled {
    pub first_person: bool,
    pub third_person_back: bool,
    pub third_person_front: bool,
    /// Shoulder surfing takes the place of the vanilla third-person-back view.
    pub replace_third_person_back: bool,
}

impl Perspective {
    pub const ALL: [Perspective; 4] = [
        Self::FirstPerson,
        Self::ThirdPersonBack,
        Self::ThirdPersonFront,
        Self::ShoulderSurfing,
    ];

    pub fn from_index(index: usize) -> Self {
        Self::ALL[index.min(3)]
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The perspective the game is in, from its camera type and our flag.
    pub fn current(camera: CameraType, surfing: bool) -> Self {
        match camera {
            CameraType::FirstPerson => Self::FirstPerson,
            CameraType::ThirdPersonFront => Self::ThirdPersonFront,
            _ if surfing => Self::ShoulderSurfing,
            _ => Self::ThirdPersonBack,
        }
    }

    /// The game's camera type for it; shoulder surfing is the back view with our flag set.
    pub fn camera_type(self) -> CameraType {
        match self {
            Self::FirstPerson => CameraType::FirstPerson,
            Self::ThirdPersonFront => CameraType::ThirdPersonFront,
            Self::ThirdPersonBack | Self::ShoulderSurfing => CameraType::ThirdPersonBack,
        }
    }

    pub fn is_enabled(self, e: &Enabled) -> bool {
        match self {
            Self::FirstPerson => e.first_person,
            Self::ThirdPersonBack => e.third_person_back && !e.replace_third_person_back,
            Self::ThirdPersonFront => e.third_person_front,
            Self::ShoulderSurfing => true,
        }
    }

    fn step(self, replace: bool) -> Self {
        if replace {
            match self {
                Self::FirstPerson | Self::ThirdPersonBack => Self::ShoulderSurfing,
                Self::ShoulderSurfing => Self::ThirdPersonFront,
                Self::ThirdPersonFront => Self::FirstPerson,
            }
        } else {
            match self {
                Self::FirstPerson => Self::ThirdPersonBack,
                Self::ThirdPersonBack => Self::ThirdPersonFront,
                Self::ThirdPersonFront => Self::ShoulderSurfing,
                Self::ShoulderSurfing => Self::FirstPerson,
            }
        }
    }

    /// `Perspective.next`: the next enabled perspective.
    pub fn next(self, e: &Enabled) -> Self {
        let mut candidate = self;
        for _ in 0..4 {
            candidate = candidate.step(e.replace_third_person_back);
            if candidate.is_enabled(e) {
                return candidate;
            }
        }
        // Shoulder surfing is always enabled, so the loop above returns before this.
        Self::ShoulderSurfing
    }

    /// The perspective to start in: `wanted` when enabled, else the next enabled one.
    pub fn or_next_enabled(self, e: &Enabled) -> Self {
        if self.is_enabled(e) {
            self
        } else {
            self.next(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ON: Enabled = Enabled {
        first_person: true,
        third_person_back: true,
        third_person_front: true,
        replace_third_person_back: false,
    };

    #[test]
    fn full_cycle() {
        let mut p = Perspective::FirstPerson;
        let mut seen = Vec::new();
        for _ in 0..4 {
            p = p.next(&ALL_ON);
            seen.push(p);
        }
        assert_eq!(
            seen,
            [
                Perspective::ThirdPersonBack,
                Perspective::ThirdPersonFront,
                Perspective::ShoulderSurfing,
                Perspective::FirstPerson
            ]
        );
    }

    #[test]
    fn replaced_back_view() {
        let e = Enabled {
            replace_third_person_back: true,
            ..ALL_ON
        };
        assert_eq!(
            Perspective::FirstPerson.next(&e),
            Perspective::ShoulderSurfing
        );
        assert_eq!(
            Perspective::ShoulderSurfing.next(&e),
            Perspective::ThirdPersonFront
        );
        assert_eq!(
            Perspective::ThirdPersonFront.next(&e),
            Perspective::FirstPerson
        );
        assert_eq!(
            Perspective::ThirdPersonBack.next(&e),
            Perspective::ShoulderSurfing
        );
    }

    #[test]
    fn disabled_are_skipped() {
        let e = Enabled {
            first_person: false,
            third_person_front: false,
            ..ALL_ON
        };
        assert_eq!(
            Perspective::ShoulderSurfing.next(&e),
            Perspective::ThirdPersonBack
        );
        assert_eq!(
            Perspective::ThirdPersonBack.next(&e),
            Perspective::ShoulderSurfing
        );
        let only = Enabled {
            first_person: false,
            third_person_back: false,
            third_person_front: false,
            replace_third_person_back: false,
        };
        assert_eq!(
            Perspective::ShoulderSurfing.next(&only),
            Perspective::ShoulderSurfing
        );
        assert_eq!(
            Perspective::FirstPerson.or_next_enabled(&only),
            Perspective::ShoulderSurfing
        );
        assert_eq!(
            Perspective::FirstPerson.or_next_enabled(&ALL_ON),
            Perspective::FirstPerson
        );
    }

    #[test]
    fn camera_type_round_trip() {
        for p in Perspective::ALL {
            let surfing = p == Perspective::ShoulderSurfing;
            assert_eq!(Perspective::current(p.camera_type(), surfing), p);
            assert_eq!(Perspective::from_index(p.index()), p);
        }
        assert_eq!(Perspective::from_index(99), Perspective::ShoulderSurfing);
    }
}
