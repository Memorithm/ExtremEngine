//! Deterministic dynamic-resolution policy with bounded changes and hysteresis.
//!
//! This controller consumes already-measured frame-budget ratios. It does not
//! infer GPU time, device class, or user-agent capability.

use crate::frame::FrameAssessment;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrsConfig {
    pub min_scale: f32,
    pub max_scale: f32,
    pub step: f32,
    pub down_threshold: f64,
    pub up_threshold: f64,
    pub consecutive_frames: u32,
    pub cooldown_frames: u32,
}

impl Default for DrsConfig {
    fn default() -> Self {
        Self {
            min_scale: 0.5,
            max_scale: 1.0,
            step: 0.05,
            down_threshold: 1.05,
            up_threshold: 0.80,
            consecutive_frames: 3,
            cooldown_frames: 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrsError {
    InvalidConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrsDecision {
    pub previous_scale: f32,
    pub next_scale: f32,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrsController {
    config: DrsConfig,
    scale: f32,
    over_streak: u32,
    under_streak: u32,
    cooldown_remaining: u32,
}

impl DrsController {
    pub fn new(config: DrsConfig, initial_scale: f32) -> Result<Self, DrsError> {
        let valid = config.min_scale.is_finite()
            && config.max_scale.is_finite()
            && config.step.is_finite()
            && config.min_scale > 0.0
            && config.min_scale <= config.max_scale
            && config.step > 0.0
            && config.down_threshold.is_finite()
            && config.up_threshold.is_finite()
            && config.up_threshold > 0.0
            && config.up_threshold < config.down_threshold
            && config.consecutive_frames > 0
            && initial_scale.is_finite()
            && initial_scale >= config.min_scale
            && initial_scale <= config.max_scale;
        if !valid {
            return Err(DrsError::InvalidConfig);
        }
        Ok(Self {
            config,
            scale: initial_scale,
            over_streak: 0,
            under_streak: 0,
            cooldown_remaining: 0,
        })
    }

    #[must_use]
    pub const fn scale(self) -> f32 {
        self.scale
    }

    pub fn observe(&mut self, assessment: FrameAssessment) -> DrsDecision {
        let previous_scale = self.scale;

        if self.cooldown_remaining > 0 {
            self.cooldown_remaining -= 1;
            self.over_streak = 0;
            self.under_streak = 0;
            return DrsDecision {
                previous_scale,
                next_scale: self.scale,
                changed: false,
            };
        }

        let pressure = assessment.gpu_ratio.unwrap_or(assessment.cpu_ratio);
        if pressure >= self.config.down_threshold {
            self.over_streak = self.over_streak.saturating_add(1);
            self.under_streak = 0;
        } else if pressure <= self.config.up_threshold {
            self.under_streak = self.under_streak.saturating_add(1);
            self.over_streak = 0;
        } else {
            self.over_streak = 0;
            self.under_streak = 0;
        }

        if self.over_streak >= self.config.consecutive_frames {
            self.scale = (self.scale - self.config.step).max(self.config.min_scale);
            self.over_streak = 0;
            self.cooldown_remaining = self.config.cooldown_frames;
        } else if self.under_streak >= self.config.consecutive_frames {
            self.scale = (self.scale + self.config.step).min(self.config.max_scale);
            self.under_streak = 0;
            self.cooldown_remaining = self.config.cooldown_frames;
        }

        DrsDecision {
            previous_scale,
            next_scale: self.scale,
            changed: self.scale != previous_scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{BudgetState, FrameAssessment};
    use core::time::Duration;

    fn assessment(cpu_ratio: f64, gpu_ratio: Option<f64>) -> FrameAssessment {
        FrameAssessment {
            state: if cpu_ratio > 1.0 {
                BudgetState::OverBudget
            } else {
                BudgetState::WithinBudget
            },
            cpu_ratio,
            gpu_ratio,
            slack: Duration::ZERO,
            overrun: Duration::ZERO,
        }
    }

    #[test]
    fn requires_hysteresis_before_lowering_scale() {
        let mut controller = DrsController::new(DrsConfig::default(), 1.0).unwrap();
        assert!(!controller.observe(assessment(1.2, None)).changed);
        assert!(!controller.observe(assessment(1.2, None)).changed);
        let decision = controller.observe(assessment(1.2, None));
        assert!(decision.changed);
        assert!((decision.next_scale - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn cooldown_prevents_immediate_repeated_changes() {
        let mut controller = DrsController::new(DrsConfig::default(), 1.0).unwrap();
        for _ in 0..3 {
            controller.observe(assessment(1.2, None));
        }
        for _ in 0..6 {
            assert!(!controller.observe(assessment(1.2, None)).changed);
        }
    }

    #[test]
    fn gpu_measurement_governs_when_available() {
        let mut controller = DrsController::new(DrsConfig::default(), 1.0).unwrap();
        for _ in 0..3 {
            controller.observe(assessment(0.5, Some(1.2)));
        }
        assert!(controller.scale() < 1.0);
    }

    #[test]
    fn bounded_scale_never_exceeds_limits() {
        let config = DrsConfig {
            consecutive_frames: 1,
            cooldown_frames: 0,
            ..DrsConfig::default()
        };
        let mut controller = DrsController::new(config, 0.5).unwrap();
        controller.observe(assessment(2.0, None));
        assert_eq!(controller.scale(), 0.5);
        let mut controller = DrsController::new(config, 1.0).unwrap();
        controller.observe(assessment(0.1, None));
        assert_eq!(controller.scale(), 1.0);
    }

    #[test]
    fn invalid_hysteresis_config_is_rejected() {
        let config = DrsConfig {
            up_threshold: 1.1,
            down_threshold: 1.0,
            ..DrsConfig::default()
        };
        assert_eq!(DrsController::new(config, 1.0), Err(DrsError::InvalidConfig));
    }
}
