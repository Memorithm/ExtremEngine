use std::time::Duration;

use extrem_app::frame::{FrameAssessment, FrameBudget, FrameSample, FrameStats as QualityStats};
use extrem_app::quality::{DrsConfig, DrsController, DrsDecision, scaled_extent};

/// CPU-side quality loop owned by the engine facade.
///
/// GPU time is not inferred: DRS uses the CPU ratio until a presenter supplies
/// a real GPU sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityLoop {
    budget: FrameBudget,
    stats: QualityStats,
    drs: DrsController,
    last_assessment: Option<FrameAssessment>,
    last_decision: Option<DrsDecision>,
}

impl QualityLoop {
    pub fn from_target_delta(delta_seconds: f32) -> Self {
        let hz = if delta_seconds.is_finite() && delta_seconds > 0.0 {
            1.0 / f64::from(delta_seconds)
        } else {
            60.0
        };
        let budget = FrameBudget::from_hz(hz).unwrap_or_else(|_| {
            FrameBudget::from_hz(60.0).expect("60 Hz is a valid fallback budget")
        });
        Self {
            budget,
            stats: QualityStats::default(),
            drs: DrsController::new(DrsConfig::default(), 1.0)
                .expect("default DRS config is valid"),
            last_assessment: None,
            last_decision: None,
        }
    }

    pub fn observe_cpu(&mut self, cpu_frame: Duration) -> DrsDecision {
        let sample = FrameSample {
            cpu_frame,
            gpu_frame: None,
        };
        self.stats.observe(self.budget, sample);
        let assessment = self.budget.assess(sample);
        let decision = self.drs.observe(assessment);
        self.last_assessment = Some(assessment);
        self.last_decision = Some(decision);
        decision
    }

    #[must_use]
    pub const fn budget(self) -> FrameBudget {
        self.budget
    }

    #[must_use]
    pub const fn stats(self) -> QualityStats {
        self.stats
    }

    #[must_use]
    pub const fn scale(self) -> f32 {
        self.drs.scale()
    }

    #[must_use]
    pub fn scaled_extent(self, width: u32, height: u32) -> Option<(u32, u32)> {
        scaled_extent(width, height, self.scale())
    }

    #[must_use]
    pub const fn last_assessment(self) -> Option<FrameAssessment> {
        self.last_assessment
    }

    #[must_use]
    pub const fn last_decision(self) -> Option<DrsDecision> {
        self.last_decision
    }
}

#[cfg(test)]
mod tests {
    use super::QualityLoop;
    use std::time::Duration;

    #[test]
    fn sixty_hertz_target_builds_a_budget() {
        let loop_ = QualityLoop::from_target_delta(1.0 / 60.0);
        assert_eq!(loop_.budget().target_hz_millihz(), 60_000);
        assert_eq!(loop_.scale(), 1.0);
        assert_eq!(loop_.scaled_extent(1280, 720), Some((1280, 720)));
    }

    #[test]
    fn repeated_overruns_eventually_drop_scale() {
        let mut loop_ = QualityLoop::from_target_delta(1.0 / 60.0);
        let heavy = Duration::from_millis(30);
        let mut changed = false;
        for _ in 0..8 {
            changed |= loop_.observe_cpu(heavy).changed;
        }
        assert!(changed);
        assert!(loop_.scale() < 1.0);
        assert!(loop_.stats().over_budget_fraction() > 0.0);
        let extent = loop_.scaled_extent(1920, 1080).expect("extent");
        assert!(extent.0 < 1920 || extent.1 < 1080);
    }
}
