//! Deterministic LOD selection from explicit scene observations.
//!
//! This policy does not infer device class or GPU cost. It starts from the
//! browser-independent signals required by the ExtremEngine roadmap: screen
//! error, distance, screen occupancy and visibility, with bounded one-level
//! changes and hysteresis.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodConfig {
    pub max_level: u8,
    pub promote_error: f32,
    pub demote_error: f32,
    pub min_occupancy_for_detail: f32,
    pub far_distance: f32,
    pub consecutive_frames: u32,
}

impl Default for LodConfig {
    fn default() -> Self {
        Self {
            max_level: 4,
            promote_error: 0.03,
            demote_error: 0.01,
            min_occupancy_for_detail: 0.02,
            far_distance: 100.0,
            consecutive_frames: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LodError {
    InvalidConfig,
    InvalidObservation,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodObservation {
    /// Normalized projected error: larger values require more detail.
    pub screen_error: f32,
    /// Camera-space distance in caller-defined world units.
    pub distance: f32,
    /// Fraction of screen area occupied by the object in `[0, 1]`.
    pub occupancy: f32,
    pub visible: bool,
}

impl LodObservation {
    fn validate(self) -> Result<(), LodError> {
        let valid = self.screen_error.is_finite()
            && self.screen_error >= 0.0
            && self.distance.is_finite()
            && self.distance >= 0.0
            && self.occupancy.is_finite()
            && (0.0..=1.0).contains(&self.occupancy);
        if valid {
            Ok(())
        } else {
            Err(LodError::InvalidObservation)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LodDecision {
    pub previous_level: u8,
    pub next_level: u8,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodSelector {
    config: LodConfig,
    level: u8,
    promote_streak: u32,
    demote_streak: u32,
}

impl LodSelector {
    pub fn new(config: LodConfig, initial_level: u8) -> Result<Self, LodError> {
        let valid = config.promote_error.is_finite()
            && config.demote_error.is_finite()
            && config.demote_error >= 0.0
            && config.promote_error > config.demote_error
            && config.min_occupancy_for_detail.is_finite()
            && (0.0..=1.0).contains(&config.min_occupancy_for_detail)
            && config.far_distance.is_finite()
            && config.far_distance > 0.0
            && config.consecutive_frames > 0
            && initial_level <= config.max_level;
        if !valid {
            return Err(LodError::InvalidConfig);
        }
        Ok(Self {
            config,
            level: initial_level,
            promote_streak: 0,
            demote_streak: 0,
        })
    }

    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }

    pub fn observe(&mut self, observation: LodObservation) -> Result<LodDecision, LodError> {
        observation.validate()?;
        let previous_level = self.level;

        let should_demote = !observation.visible
            || observation.distance >= self.config.far_distance
            || observation.occupancy < self.config.min_occupancy_for_detail
            || observation.screen_error <= self.config.demote_error;
        let should_promote = observation.visible
            && observation.distance < self.config.far_distance
            && observation.occupancy >= self.config.min_occupancy_for_detail
            && observation.screen_error >= self.config.promote_error;

        if should_promote {
            self.promote_streak = self.promote_streak.saturating_add(1);
            self.demote_streak = 0;
        } else if should_demote {
            self.demote_streak = self.demote_streak.saturating_add(1);
            self.promote_streak = 0;
        } else {
            self.promote_streak = 0;
            self.demote_streak = 0;
        }

        if self.promote_streak >= self.config.consecutive_frames && self.level > 0 {
            self.level -= 1;
            self.promote_streak = 0;
        } else if self.demote_streak >= self.config.consecutive_frames
            && self.level < self.config.max_level
        {
            self.level += 1;
            self.demote_streak = 0;
        }

        Ok(LodDecision {
            previous_level,
            next_level: self.level,
            changed: self.level != previous_level,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(error: f32, distance: f32, occupancy: f32, visible: bool) -> LodObservation {
        LodObservation {
            screen_error: error,
            distance,
            occupancy,
            visible,
        }
    }

    #[test]
    fn promotion_requires_hysteresis_and_moves_one_level() {
        let mut selector = LodSelector::new(LodConfig::default(), 2).unwrap();
        for _ in 0..2 {
            assert!(!selector.observe(observation(0.05, 10.0, 0.2, true)).unwrap().changed);
        }
        let decision = selector.observe(observation(0.05, 10.0, 0.2, true)).unwrap();
        assert_eq!(decision.next_level, 1);
    }

    #[test]
    fn invisible_objects_demote_after_hysteresis() {
        let mut selector = LodSelector::new(LodConfig::default(), 1).unwrap();
        for _ in 0..3 {
            selector.observe(observation(0.2, 5.0, 0.5, false)).unwrap();
        }
        assert_eq!(selector.level(), 2);
    }

    #[test]
    fn neutral_band_resets_streaks() {
        let mut selector = LodSelector::new(LodConfig::default(), 2).unwrap();
        selector.observe(observation(0.05, 10.0, 0.2, true)).unwrap();
        selector.observe(observation(0.02, 10.0, 0.2, true)).unwrap();
        selector.observe(observation(0.05, 10.0, 0.2, true)).unwrap();
        selector.observe(observation(0.05, 10.0, 0.2, true)).unwrap();
        assert_eq!(selector.level(), 2);
    }

    #[test]
    fn levels_remain_bounded() {
        let config = LodConfig {
            consecutive_frames: 1,
            ..LodConfig::default()
        };
        let mut finest = LodSelector::new(config, 0).unwrap();
        finest.observe(observation(0.1, 1.0, 1.0, true)).unwrap();
        assert_eq!(finest.level(), 0);
        let mut coarsest = LodSelector::new(config, config.max_level).unwrap();
        coarsest.observe(observation(0.0, 1_000.0, 0.0, false)).unwrap();
        assert_eq!(coarsest.level(), config.max_level);
    }

    #[test]
    fn invalid_observation_fails_closed() {
        let mut selector = LodSelector::new(LodConfig::default(), 1).unwrap();
        assert_eq!(
            selector.observe(observation(f32::NAN, 1.0, 0.5, true)),
            Err(LodError::InvalidObservation)
        );
    }
}
