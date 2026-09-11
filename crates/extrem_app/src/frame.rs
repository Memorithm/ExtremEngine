//! Deterministic frame-budget accounting for native and Web/WASM render loops.
//!
//! GPU time is optional and only participates when a caller has a real
//! measurement source. CPU frame time remains explicit.

use core::fmt;
use core::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    InvalidTargetHz,
    DurationOverflow,
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTargetHz => formatter.write_str("target_hz must be finite and > 0"),
            Self::DurationOverflow => formatter.write_str("duration is too large to represent"),
        }
    }
}

impl std::error::Error for FrameError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameBudget {
    target_hz_millihz: u64,
    budget: Duration,
}

impl FrameBudget {
    pub fn from_hz(target_hz: f64) -> Result<Self, FrameError> {
        if !target_hz.is_finite() || target_hz <= 0.0 {
            return Err(FrameError::InvalidTargetHz);
        }
        let millihz = (target_hz * 1000.0).round();
        if millihz < 1.0 || millihz > u64::MAX as f64 {
            return Err(FrameError::InvalidTargetHz);
        }
        let target_hz_millihz = millihz as u64;
        let budget_ns = 1_000_000_000_000u128 / u128::from(target_hz_millihz);
        let budget_ns = u64::try_from(budget_ns).map_err(|_| FrameError::DurationOverflow)?;
        Ok(Self {
            target_hz_millihz,
            budget: Duration::from_nanos(budget_ns),
        })
    }

    #[must_use]
    pub const fn target_hz_millihz(self) -> u64 {
        self.target_hz_millihz
    }

    #[must_use]
    pub const fn duration(self) -> Duration {
        self.budget
    }

    #[must_use]
    pub fn assess(self, sample: FrameSample) -> FrameAssessment {
        let budget_ns = self.budget.as_nanos() as f64;
        let cpu_ns = sample.cpu_frame.as_nanos() as f64;
        let gpu_ratio = sample
            .gpu_frame
            .map(|duration| duration.as_nanos() as f64 / budget_ns);
        let over = sample.cpu_frame > self.budget;
        FrameAssessment {
            state: if over {
                BudgetState::OverBudget
            } else {
                BudgetState::WithinBudget
            },
            cpu_ratio: cpu_ns / budget_ns,
            gpu_ratio,
            slack: self.budget.saturating_sub(sample.cpu_frame),
            overrun: sample.cpu_frame.saturating_sub(self.budget),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSample {
    pub cpu_frame: Duration,
    pub gpu_frame: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetState {
    WithinBudget,
    OverBudget,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameAssessment {
    pub state: BudgetState,
    pub cpu_ratio: f64,
    pub gpu_ratio: Option<f64>,
    pub slack: Duration,
    pub overrun: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStats {
    count: u64,
    mean_ms: f64,
    m2_ms2: f64,
    over_budget: u64,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            count: 0,
            mean_ms: 0.0,
            m2_ms2: 0.0,
            over_budget: 0,
        }
    }
}

impl FrameStats {
    pub fn observe(&mut self, budget: FrameBudget, sample: FrameSample) {
        let value_ms = sample.cpu_frame.as_secs_f64() * 1000.0;
        self.count += 1;
        let delta = value_ms - self.mean_ms;
        self.mean_ms += delta / self.count as f64;
        let delta2 = value_ms - self.mean_ms;
        self.m2_ms2 += delta * delta2;
        if sample.cpu_frame > budget.duration() {
            self.over_budget += 1;
        }
    }

    #[must_use]
    pub const fn count(self) -> u64 {
        self.count
    }

    #[must_use]
    pub const fn mean_ms(self) -> f64 {
        self.mean_ms
    }

    #[must_use]
    pub fn jitter_stddev_ms(self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            (self.m2_ms2 / self.count as f64).sqrt()
        }
    }

    #[must_use]
    pub fn over_budget_fraction(self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.over_budget as f64 / self.count as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_hz_budget_is_about_sixteen_point_six_ms() {
        let budget = FrameBudget::from_hz(60.0).unwrap();
        assert_eq!(budget.target_hz_millihz(), 60_000);
        assert_eq!(budget.duration(), Duration::from_nanos(16_666_666));
    }

    #[test]
    fn rejects_invalid_rates() {
        assert_eq!(FrameBudget::from_hz(0.0), Err(FrameError::InvalidTargetHz));
        assert_eq!(
            FrameBudget::from_hz(f64::NAN),
            Err(FrameError::InvalidTargetHz)
        );
    }

    #[test]
    fn gpu_ratio_is_absent_when_not_measured() {
        let budget = FrameBudget::from_hz(60.0).unwrap();
        let assessment = budget.assess(FrameSample {
            cpu_frame: Duration::from_millis(10),
            gpu_frame: None,
        });
        assert_eq!(assessment.state, BudgetState::WithinBudget);
        assert_eq!(assessment.gpu_ratio, None);
        assert!(assessment.slack > Duration::ZERO);
        assert_eq!(assessment.overrun, Duration::ZERO);
    }

    #[test]
    fn overrun_is_explicit() {
        let budget = FrameBudget::from_hz(60.0).unwrap();
        let assessment = budget.assess(FrameSample {
            cpu_frame: Duration::from_millis(20),
            gpu_frame: Some(Duration::from_millis(15)),
        });
        assert_eq!(assessment.state, BudgetState::OverBudget);
        assert!(assessment.overrun > Duration::ZERO);
        assert!(assessment.gpu_ratio.unwrap() < 1.0);
    }

    #[test]
    fn streaming_stats_track_jitter_and_misses() {
        let budget = FrameBudget::from_hz(60.0).unwrap();
        let mut stats = FrameStats::default();
        for millis in [10, 12, 20, 14] {
            stats.observe(
                budget,
                FrameSample {
                    cpu_frame: Duration::from_millis(millis),
                    gpu_frame: None,
                },
            );
        }
        assert_eq!(stats.count(), 4);
        assert!((stats.mean_ms() - 14.0).abs() < 1e-12);
        assert!(stats.jitter_stddev_ms() > 0.0);
        assert_eq!(stats.over_budget_fraction(), 0.25);
    }
}
