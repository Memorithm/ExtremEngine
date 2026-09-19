//! BE15e controlled real-consumer effect benchmark for ElasticXxx adaptive quality.
//!
//! This exercises the real ExtremEngine fixed-step scheduler with `NullRenderer`.
//! It records scheduler effects and host elapsed time, but establishes no FPS, GPU,
//! visual-quality, energy, or end-to-end performance claim.

use extrem_engine::{
    AdaptiveFixedStepController, AdaptiveQualityConfig, AdaptiveQualityOutcome, Engine,
    FrameTimeObservation, Stage,
};
use std::hint::black_box;
use std::time::Instant;

const WARMUP: usize = 3;
const SAMPLES: usize = 21;
const FRAMES: u64 = 120;
const FIXED_DELTA_SECONDS: f32 = 1.0 / 60.0;
const FRAME_DELTA_SECONDS: f32 = 0.050;

#[derive(Clone, Copy)]
struct Scenario {
    name: &'static str,
    initial_budget: u32,
    observed_seconds: f32,
}

const SCENARIOS: [Scenario; 2] = [
    Scenario {
        name: "pressure",
        initial_budget: 4,
        observed_seconds: 0.040,
    },
    Scenario {
        name: "recovery",
        initial_budget: 2,
        observed_seconds: 0.010,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Effect {
    fixed_steps: u64,
    debt_frames: u64,
    transitions: u64,
    final_budget: u32,
    workload_fingerprint: u64,
}

fn config() -> AdaptiveQualityConfig {
    AdaptiveQualityConfig {
        degrade_above_seconds: 0.030,
        recover_below_seconds: 0.015,
        cooldown_frames: 10,
        min_fixed_steps_per_frame: 2,
        max_fixed_steps_per_frame: 4,
    }
}

fn engine(initial_budget: u32) -> Engine {
    let mut engine = Engine::new();
    engine
        .app_mut()
        .set_fixed_timestep(FIXED_DELTA_SECONDS)
        .set_max_fixed_steps_per_frame(initial_budget);
    engine.set_max_fixed_steps_per_frame(initial_budget);
    engine.world_mut().insert_resource(0_u64);
    engine
        .app_mut()
        .add_systems(Stage::FixedUpdate, |world, time| {
            let state = world
                .get_resource_mut::<u64>()
                .expect("benchmark workload state is installed");
            *state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(time.fixed_step ^ 1_442_695_040_888_963_407);
        });
    engine
}

fn run_baseline(scenario: Scenario) -> Result<Effect, Box<dyn std::error::Error>> {
    let mut engine = engine(scenario.initial_budget);
    let mut debt_frames = 0_u64;
    for _ in 0..FRAMES {
        let report = black_box(&mut engine).tick(black_box(FRAME_DELTA_SECONDS))?;
        debt_frames += u64::from(report.fixed_debt_dropped);
    }
    Ok(Effect {
        fixed_steps: engine.app().time().fixed_step,
        debt_frames,
        transitions: 0,
        final_budget: engine.max_fixed_steps_per_frame(),
        workload_fingerprint: *engine
            .world()
            .get_resource::<u64>()
            .expect("benchmark workload state remains installed"),
    })
}

fn run_adaptive(scenario: Scenario) -> Result<Effect, Box<dyn std::error::Error>> {
    let mut engine = engine(scenario.initial_budget);
    let mut controller = AdaptiveFixedStepController::new(config())?;
    let mut debt_frames = 0_u64;
    let mut transitions = 0_u64;
    for frame in 1..=FRAMES {
        let outcome = controller.apply(
            &mut engine,
            FrameTimeObservation::measured(frame, scenario.observed_seconds),
        );
        if matches!(outcome, AdaptiveQualityOutcome::Committed { .. }) {
            transitions += 1;
        }
        let report = black_box(&mut engine).tick(black_box(FRAME_DELTA_SECONDS))?;
        debt_frames += u64::from(report.fixed_debt_dropped);
    }
    Ok(Effect {
        fixed_steps: engine.app().time().fixed_step,
        debt_frames,
        transitions,
        final_budget: engine.max_fixed_steps_per_frame(),
        workload_fingerprint: *engine
            .world()
            .get_resource::<u64>()
            .expect("benchmark workload state remains installed"),
    })
}

fn validate_effects(scenario: Scenario, baseline: Effect, adaptive: Effect) {
    assert_eq!(baseline.final_budget, scenario.initial_budget);
    assert_eq!(adaptive.transitions, 2);
    match scenario.name {
        "pressure" => {
            assert_eq!(adaptive.final_budget, 2);
            assert!(adaptive.fixed_steps < baseline.fixed_steps);
            assert!(adaptive.debt_frames > baseline.debt_frames);
        }
        "recovery" => {
            assert_eq!(adaptive.final_budget, 4);
            assert!(adaptive.fixed_steps > baseline.fixed_steps);
            assert!(adaptive.debt_frames < baseline.debt_frames);
        }
        _ => unreachable!("all benchmark scenarios are declared statically"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let revision = std::env::var("EXTREM_BENCH_REVISION")?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("EXTREM_BENCH_REVISION must be the checked-out 40-digit Git SHA".into());
    }

    println!("# code_revision={revision}");
    println!("# elastic_revision=8441991feea3a2aae19f62a8e51c89e7f0d6f969");
    println!(
        "# backend=NullRenderer,fixed_delta_seconds={FIXED_DELTA_SECONDS},frame_delta_seconds={FRAME_DELTA_SECONDS}"
    );
    println!("# warmup={WARMUP},samples={SAMPLES},frames={FRAMES},time_unit=nanoseconds");
    println!(
        "# elapsed time is observational; modes execute different fixed-step counts by design"
    );
    println!(
        "# raw,scenario,mode,sample,total_ns,fixed_steps,debt_frames,transitions,final_budget,workload_fingerprint"
    );
    println!(
        "# summary,scenario,mode,median_total_ns,p95_total_ns,fixed_steps,debt_frames,transitions,final_budget"
    );

    for scenario in SCENARIOS {
        let baseline_effect = run_baseline(scenario)?;
        let adaptive_effect = run_adaptive(scenario)?;
        validate_effects(scenario, baseline_effect, adaptive_effect);

        let mut baseline_samples = Vec::with_capacity(SAMPLES);
        let mut adaptive_samples = Vec::with_capacity(SAMPLES);
        for sample in 0..(WARMUP + SAMPLES) {
            let adaptive_first = sample % 2 == 1;
            let execute = |mode: &str| -> Result<(u128, Effect), Box<dyn std::error::Error>> {
                let start = Instant::now();
                let effect = match mode {
                    "baseline" => run_baseline(scenario)?,
                    "adaptive" => run_adaptive(scenario)?,
                    _ => unreachable!("only declared benchmark modes are executed"),
                };
                Ok((start.elapsed().as_nanos(), effect))
            };
            let order = if adaptive_first {
                ["adaptive", "baseline"]
            } else {
                ["baseline", "adaptive"]
            };
            for mode in order {
                let (elapsed, effect) = execute(mode)?;
                let expected = if mode == "baseline" {
                    baseline_effect
                } else {
                    adaptive_effect
                };
                assert_eq!(effect, expected);
                if sample >= WARMUP {
                    let sample_id = sample - WARMUP;
                    println!(
                        "raw,{},{mode},{sample_id},{elapsed},{},{},{},{},{}",
                        scenario.name,
                        effect.fixed_steps,
                        effect.debt_frames,
                        effect.transitions,
                        effect.final_budget,
                        effect.workload_fingerprint
                    );
                    if mode == "baseline" {
                        baseline_samples.push(elapsed);
                    } else {
                        adaptive_samples.push(elapsed);
                    }
                }
            }
        }

        for (mode, mut samples, effect) in [
            ("baseline", baseline_samples, baseline_effect),
            ("adaptive", adaptive_samples, adaptive_effect),
        ] {
            samples.sort_unstable();
            let median = samples[SAMPLES / 2];
            let p95 = samples[(SAMPLES * 95).div_ceil(100) - 1];
            println!(
                "summary,{},{mode},{median},{p95},{},{},{},{}",
                scenario.name,
                effect.fixed_steps,
                effect.debt_frames,
                effect.transitions,
                effect.final_budget
            );
        }
    }
    Ok(())
}
