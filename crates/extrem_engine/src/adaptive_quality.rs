//! Optional ElasticXxx-backed admission for one real ExtremEngine quality/resource knob.
//!
//! The controller never infers that a lower fixed-step budget is universally better.
//! The embedding supplies explicit frame-time thresholds and a measured frame-time
//! observation. Boolean admission only decides whether a declared candidate may reach
//! the trusted actuator. Hysteresis, cooldown, validation, verification and rollback
//! remain explicit.

use elastic::{BoolExpr, BooleanGuard, GuardScope, PredicateKey, PredicateRegistry, TruthValue};
use std::collections::BTreeMap;
use std::fmt;

const PREDICATE_NAMESPACE: &str = "extremengine.quality";
const METRIC_AVAILABLE: &str = "metric-available";
const COOLDOWN_READY: &str = "cooldown-ready";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveQualityConfig {
    pub degrade_above_seconds: f32,
    pub recover_below_seconds: f32,
    pub cooldown_frames: u64,
    pub min_fixed_steps_per_frame: u32,
    pub max_fixed_steps_per_frame: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdaptiveQualityConfigError {
    NonFiniteThreshold,
    NonPositiveThreshold,
    InvalidHysteresis,
    ZeroMinimum,
    InvalidStepRange,
}

impl fmt::Display for AdaptiveQualityConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteThreshold => formatter.write_str("frame-time thresholds must be finite"),
            Self::NonPositiveThreshold => {
                formatter.write_str("frame-time thresholds must be positive")
            }
            Self::InvalidHysteresis => formatter
                .write_str("recover_below_seconds must be strictly below degrade_above_seconds"),
            Self::ZeroMinimum => {
                formatter.write_str("minimum fixed-step budget must be at least one")
            }
            Self::InvalidStepRange => formatter
                .write_str("minimum fixed-step budget must not exceed maximum fixed-step budget"),
        }
    }
}

impl std::error::Error for AdaptiveQualityConfigError {}

impl AdaptiveQualityConfig {
    pub fn validate(self) -> Result<Self, AdaptiveQualityConfigError> {
        if !self.degrade_above_seconds.is_finite() || !self.recover_below_seconds.is_finite() {
            return Err(AdaptiveQualityConfigError::NonFiniteThreshold);
        }
        if self.degrade_above_seconds <= 0.0 || self.recover_below_seconds <= 0.0 {
            return Err(AdaptiveQualityConfigError::NonPositiveThreshold);
        }
        if self.recover_below_seconds >= self.degrade_above_seconds {
            return Err(AdaptiveQualityConfigError::InvalidHysteresis);
        }
        if self.min_fixed_steps_per_frame == 0 {
            return Err(AdaptiveQualityConfigError::ZeroMinimum);
        }
        if self.min_fixed_steps_per_frame > self.max_fixed_steps_per_frame {
            return Err(AdaptiveQualityConfigError::InvalidStepRange);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTimeObservation {
    pub frame: u64,
    /// Wall-clock frame interval supplied by the embedding. `None`, non-finite and
    /// non-positive values are missing evidence and therefore evaluate as Unknown.
    pub measured_seconds: Option<f32>,
}

impl FrameTimeObservation {
    pub const fn measured(frame: u64, seconds: f32) -> Self {
        Self {
            frame,
            measured_seconds: Some(seconds),
        }
    }

    pub const fn missing(frame: u64) -> Self {
        Self {
            frame,
            measured_seconds: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdaptiveQualityDecision {
    Hold,
    DecreaseFixedStepBudget { target: u32 },
    IncreaseFixedStepBudget { target: u32 },
}

impl AdaptiveQualityDecision {
    const fn target(self) -> Option<u32> {
        match self {
            Self::Hold => None,
            Self::DecreaseFixedStepBudget { target } | Self::IncreaseFixedStepBudget { target } => {
                Some(target)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdaptiveQualityOutcome {
    Held {
        guard: TruthValue,
        decision: AdaptiveQualityDecision,
    },
    Committed {
        previous: u32,
        target: u32,
        decision: AdaptiveQualityDecision,
    },
    RejectedByTrustedValidation {
        target: u32,
    },
    RolledBackAfterVerificationFailure {
        attempted: u32,
        restored: u32,
    },
    RollbackFailedClosed {
        attempted: u32,
        expected_restore: u32,
    },
    /// A prior rollback could not be verified; later admission stays disabled.
    FaultLatched,
}

/// Trusted stateful boundary. Boolean eligibility alone never calls `apply` unless
/// `validate` succeeds immediately before mutation. A failed verification must roll
/// back and re-verify the previous value.
pub trait FixedStepBudgetActuator {
    fn current_fixed_step_budget(&self) -> u32;
    fn validate_fixed_step_budget(&self, target: u32) -> bool;
    fn apply_fixed_step_budget(&mut self, target: u32);
    fn verify_fixed_step_budget(&self, target: u32) -> bool;
    fn rollback_fixed_step_budget(&mut self, previous: u32);
}

impl<R: extrem_render::RenderBackend> FixedStepBudgetActuator for super::Engine<R> {
    fn current_fixed_step_budget(&self) -> u32 {
        self.max_fixed_steps_per_frame()
    }

    fn validate_fixed_step_budget(&self, target: u32) -> bool {
        target >= 1
    }

    fn apply_fixed_step_budget(&mut self, target: u32) {
        self.set_max_fixed_steps_per_frame(target);
    }

    fn verify_fixed_step_budget(&self, target: u32) -> bool {
        self.max_fixed_steps_per_frame() == target
            && self.config().max_fixed_steps_per_frame == target
    }

    fn rollback_fixed_step_budget(&mut self, previous: u32) {
        self.set_max_fixed_steps_per_frame(previous);
    }
}

pub struct AdaptiveFixedStepController {
    config: AdaptiveQualityConfig,
    guard: BooleanGuard,
    metric_key: PredicateKey,
    cooldown_key: PredicateKey,
    last_transition_frame: Option<u64>,
    fault_latched: bool,
}

impl AdaptiveFixedStepController {
    pub fn new(config: AdaptiveQualityConfig) -> Result<Self, AdaptiveQualityConfigError> {
        let config = config.validate()?;
        let metric_key = PredicateKey::new(PREDICATE_NAMESPACE, METRIC_AVAILABLE)
            .expect("static ExtremEngine predicate key is valid");
        let cooldown_key = PredicateKey::new(PREDICATE_NAMESPACE, COOLDOWN_READY)
            .expect("static ExtremEngine predicate key is valid");
        let registry = PredicateRegistry::from_keys([metric_key.clone(), cooldown_key.clone()])
            .expect("two static ExtremEngine predicates fit the bounded registry");
        let metric = registry
            .id(&metric_key)
            .expect("registered metric predicate has an id");
        let cooldown = registry
            .id(&cooldown_key)
            .expect("registered cooldown predicate has an id");
        let guard = BooleanGuard::when(
            GuardScope::Resource,
            registry,
            BoolExpr::all([BoolExpr::atom(metric), BoolExpr::atom(cooldown)]),
        )
        .expect("bounded static ExtremEngine guard canonicalizes");
        Ok(Self {
            config,
            guard,
            metric_key,
            cooldown_key,
            last_transition_frame: None,
            fault_latched: false,
        })
    }

    pub const fn config(&self) -> AdaptiveQualityConfig {
        self.config
    }

    pub const fn last_transition_frame(&self) -> Option<u64> {
        self.last_transition_frame
    }

    /// Whether an unverified rollback has disabled further admission.
    pub const fn fault_latched(&self) -> bool {
        self.fault_latched
    }

    /// Clear a latched rollback fault only after the trusted actuator verifies an
    /// explicitly expected in-range state. This method never mutates the actuator.
    pub fn recover_after_fault<A: FixedStepBudgetActuator>(
        &mut self,
        actuator: &A,
        expected: u32,
    ) -> bool {
        if !self.fault_latched
            || expected < self.config.min_fixed_steps_per_frame
            || expected > self.config.max_fixed_steps_per_frame
            || actuator.current_fixed_step_budget() != expected
            || !actuator.verify_fixed_step_budget(expected)
        {
            return false;
        }
        self.fault_latched = false;
        true
    }

    pub fn apply<A: FixedStepBudgetActuator>(
        &mut self,
        actuator: &mut A,
        observation: FrameTimeObservation,
    ) -> AdaptiveQualityOutcome {
        if self.fault_latched {
            return AdaptiveQualityOutcome::FaultLatched;
        }

        let current = actuator.current_fixed_step_budget();
        let measurement = observation
            .measured_seconds
            .filter(|value| value.is_finite() && *value > 0.0);
        let decision = match measurement {
            Some(seconds)
                if seconds > self.config.degrade_above_seconds
                    && current > self.config.min_fixed_steps_per_frame =>
            {
                AdaptiveQualityDecision::DecreaseFixedStepBudget {
                    target: current - 1,
                }
            }
            Some(seconds)
                if seconds < self.config.recover_below_seconds
                    && current < self.config.max_fixed_steps_per_frame =>
            {
                AdaptiveQualityDecision::IncreaseFixedStepBudget {
                    target: current + 1,
                }
            }
            _ => AdaptiveQualityDecision::Hold,
        };

        let metric_truth = if measurement.is_some() {
            TruthValue::True
        } else {
            TruthValue::Unknown
        };
        let cooldown_truth = match self.last_transition_frame {
            Some(last) if observation.frame < last => TruthValue::Unknown,
            Some(last) if observation.frame.saturating_sub(last) < self.config.cooldown_frames => {
                TruthValue::False
            }
            _ => TruthValue::True,
        };
        let facts = BTreeMap::from([
            (self.metric_key.clone(), metric_truth),
            (self.cooldown_key.clone(), cooldown_truth),
        ]);
        let guard = self
            .guard
            .evaluate(&facts)
            .expect("two registered facts cannot exceed the Boolean fast-path bounds");

        let Some(target) = decision.target() else {
            return AdaptiveQualityOutcome::Held { guard, decision };
        };
        if guard != TruthValue::True {
            return AdaptiveQualityOutcome::Held { guard, decision };
        }
        if target < self.config.min_fixed_steps_per_frame
            || target > self.config.max_fixed_steps_per_frame
            || !actuator.validate_fixed_step_budget(target)
        {
            return AdaptiveQualityOutcome::RejectedByTrustedValidation { target };
        }

        let previous = current;
        actuator.apply_fixed_step_budget(target);
        if actuator.verify_fixed_step_budget(target) {
            self.last_transition_frame = Some(observation.frame);
            return AdaptiveQualityOutcome::Committed {
                previous,
                target,
                decision,
            };
        }

        actuator.rollback_fixed_step_budget(previous);
        if actuator.verify_fixed_step_budget(previous) {
            AdaptiveQualityOutcome::RolledBackAfterVerificationFailure {
                attempted: target,
                restored: previous,
            }
        } else {
            self.fault_latched = true;
            AdaptiveQualityOutcome::RollbackFailedClosed {
                attempted: target,
                expected_restore: previous,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    fn config() -> AdaptiveQualityConfig {
        AdaptiveQualityConfig {
            degrade_above_seconds: 0.030,
            recover_below_seconds: 0.015,
            cooldown_frames: 3,
            min_fixed_steps_per_frame: 2,
            max_fixed_steps_per_frame: 4,
        }
    }

    #[test]
    fn missing_metric_is_unknown_and_never_mutates() {
        let mut controller = AdaptiveFixedStepController::new(config()).unwrap();
        let mut engine = Engine::new();
        engine.set_max_fixed_steps_per_frame(4);
        let outcome = controller.apply(&mut engine, FrameTimeObservation::missing(1));
        assert_eq!(
            outcome,
            AdaptiveQualityOutcome::Held {
                guard: TruthValue::Unknown,
                decision: AdaptiveQualityDecision::Hold,
            }
        );
        assert_eq!(engine.max_fixed_steps_per_frame(), 4);
    }

    #[test]
    fn hysteresis_and_cooldown_bound_stateful_changes() {
        let mut controller = AdaptiveFixedStepController::new(config()).unwrap();
        let mut engine = Engine::new();
        engine.set_max_fixed_steps_per_frame(4);

        assert!(matches!(
            controller.apply(&mut engine, FrameTimeObservation::measured(10, 0.040)),
            AdaptiveQualityOutcome::Committed {
                previous: 4,
                target: 3,
                ..
            }
        ));
        assert_eq!(engine.max_fixed_steps_per_frame(), 3);

        assert_eq!(
            controller.apply(&mut engine, FrameTimeObservation::measured(11, 0.040)),
            AdaptiveQualityOutcome::Held {
                guard: TruthValue::False,
                decision: AdaptiveQualityDecision::DecreaseFixedStepBudget { target: 2 },
            }
        );
        assert_eq!(engine.max_fixed_steps_per_frame(), 3);

        assert!(matches!(
            controller.apply(&mut engine, FrameTimeObservation::measured(13, 0.040)),
            AdaptiveQualityOutcome::Committed {
                previous: 3,
                target: 2,
                ..
            }
        ));
        assert_eq!(engine.max_fixed_steps_per_frame(), 2);

        assert_eq!(
            controller.apply(&mut engine, FrameTimeObservation::measured(16, 0.020)),
            AdaptiveQualityOutcome::Held {
                guard: TruthValue::True,
                decision: AdaptiveQualityDecision::Hold,
            }
        );
        assert_eq!(engine.max_fixed_steps_per_frame(), 2);

        assert!(matches!(
            controller.apply(&mut engine, FrameTimeObservation::measured(17, 0.010)),
            AdaptiveQualityOutcome::Committed {
                previous: 2,
                target: 3,
                ..
            }
        ));
    }

    #[derive(Debug)]
    struct FaultyActuator {
        current: u32,
        fail_target_verification: bool,
        fail_rollback_verification: bool,
        previous: u32,
    }

    impl FixedStepBudgetActuator for FaultyActuator {
        fn current_fixed_step_budget(&self) -> u32 {
            self.current
        }
        fn validate_fixed_step_budget(&self, target: u32) -> bool {
            target >= 1
        }
        fn apply_fixed_step_budget(&mut self, target: u32) {
            self.previous = self.current;
            self.current = target;
        }
        fn verify_fixed_step_budget(&self, target: u32) -> bool {
            if target == self.previous && self.fail_rollback_verification {
                return false;
            }
            if target == self.current && self.fail_target_verification && target != self.previous {
                return false;
            }
            self.current == target
        }
        fn rollback_fixed_step_budget(&mut self, previous: u32) {
            self.current = previous;
        }
    }

    #[test]
    fn verification_failure_rolls_back_before_cooldown_is_committed() {
        let mut controller = AdaptiveFixedStepController::new(config()).unwrap();
        let mut actuator = FaultyActuator {
            current: 4,
            previous: 4,
            fail_target_verification: true,
            fail_rollback_verification: false,
        };
        assert_eq!(
            controller.apply(&mut actuator, FrameTimeObservation::measured(3, 0.040)),
            AdaptiveQualityOutcome::RolledBackAfterVerificationFailure {
                attempted: 3,
                restored: 4,
            }
        );
        assert_eq!(actuator.current, 4);
        assert_eq!(controller.last_transition_frame(), None);
    }

    #[test]
    fn trusted_validation_rejection_never_reaches_mutation() {
        struct RejectingActuator {
            current: u32,
        }

        impl FixedStepBudgetActuator for RejectingActuator {
            fn current_fixed_step_budget(&self) -> u32 {
                self.current
            }

            fn validate_fixed_step_budget(&self, _target: u32) -> bool {
                false
            }

            fn apply_fixed_step_budget(&mut self, _target: u32) {
                panic!("trusted validation rejection must prevent actuation");
            }

            fn verify_fixed_step_budget(&self, target: u32) -> bool {
                self.current == target
            }

            fn rollback_fixed_step_budget(&mut self, _previous: u32) {
                panic!("no rollback is needed when validation rejects before actuation");
            }
        }

        let mut controller = AdaptiveFixedStepController::new(config()).unwrap();
        let mut actuator = RejectingActuator { current: 4 };
        assert_eq!(
            controller.apply(&mut actuator, FrameTimeObservation::measured(3, 0.040)),
            AdaptiveQualityOutcome::RejectedByTrustedValidation { target: 3 }
        );
        assert_eq!(actuator.current, 4);
        assert_eq!(controller.last_transition_frame(), None);
    }

    #[test]
    fn rollback_verification_failure_is_explicitly_fail_closed() {
        let mut controller = AdaptiveFixedStepController::new(config()).unwrap();
        let mut actuator = FaultyActuator {
            current: 4,
            previous: 4,
            fail_target_verification: true,
            fail_rollback_verification: true,
        };
        assert_eq!(
            controller.apply(&mut actuator, FrameTimeObservation::measured(3, 0.040)),
            AdaptiveQualityOutcome::RollbackFailedClosed {
                attempted: 3,
                expected_restore: 4,
            }
        );
        assert_eq!(controller.last_transition_frame(), None);
        assert!(controller.fault_latched());

        actuator.fail_target_verification = false;
        actuator.fail_rollback_verification = false;
        assert_eq!(
            controller.apply(&mut actuator, FrameTimeObservation::measured(4, 0.040)),
            AdaptiveQualityOutcome::FaultLatched
        );
        assert_eq!(actuator.current, 4);

        assert!(controller.recover_after_fault(&actuator, 4));
        assert!(!controller.fault_latched());
        assert!(matches!(
            controller.apply(&mut actuator, FrameTimeObservation::measured(5, 0.040)),
            AdaptiveQualityOutcome::Committed {
                previous: 4,
                target: 3,
                ..
            }
        ));
    }

    #[test]
    fn fault_recovery_requires_verified_expected_state() {
        let mut controller = AdaptiveFixedStepController::new(config()).unwrap();
        let mut actuator = FaultyActuator {
            current: 4,
            previous: 4,
            fail_target_verification: true,
            fail_rollback_verification: true,
        };
        assert!(matches!(
            controller.apply(&mut actuator, FrameTimeObservation::measured(3, 0.040)),
            AdaptiveQualityOutcome::RollbackFailedClosed { .. }
        ));
        assert!(!controller.recover_after_fault(&actuator, 4));
        assert!(controller.fault_latched());
    }

    #[test]
    fn out_of_order_frame_is_unknown_and_cannot_mutate() {
        let mut controller = AdaptiveFixedStepController::new(config()).unwrap();
        let mut engine = Engine::new();
        engine.set_max_fixed_steps_per_frame(4);
        assert!(matches!(
            controller.apply(&mut engine, FrameTimeObservation::measured(10, 0.040)),
            AdaptiveQualityOutcome::Committed { target: 3, .. }
        ));
        assert_eq!(
            controller.apply(&mut engine, FrameTimeObservation::measured(9, 0.040)),
            AdaptiveQualityOutcome::Held {
                guard: TruthValue::Unknown,
                decision: AdaptiveQualityDecision::DecreaseFixedStepBudget { target: 2 },
            }
        );
        assert_eq!(engine.max_fixed_steps_per_frame(), 3);
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        let mut invalid = config();
        invalid.recover_below_seconds = invalid.degrade_above_seconds;
        assert!(matches!(
            AdaptiveFixedStepController::new(invalid),
            Err(AdaptiveQualityConfigError::InvalidHysteresis)
        ));
    }
}
