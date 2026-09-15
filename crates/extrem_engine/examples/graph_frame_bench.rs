//! EE-PERF-04: identical harness for old/new complete headless CPU ticks.
//! This file uses only APIs present at the frozen reference revision.
use extrem_engine::Engine;
use extrem_math::{Transform, Vec3};
use extrem_render::{RenderGraph, RenderGraphError};
use std::hint::black_box;
use std::time::Instant;

fn graph(passes: usize, epoch: usize) -> Result<(RenderGraph, Vec<String>), RenderGraphError> {
    let mut graph = RenderGraph::new();
    let names: Vec<_> = (0..passes)
        .map(|index| format!("epoch-{epoch}-pass-{index:05}"))
        .collect();
    let ids: Vec<_> = names.iter().map(|name| graph.add_pass(name)).collect();
    for pair in ids.windows(2) {
        graph.add_dependency(pair[1], pair[0])?;
    }
    Ok((graph, names))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let revision = std::env::var("EXTREM_BENCH_REVISION")?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("EXTREM_BENCH_REVISION must be the checked-out 40-digit SHA".into());
    }
    const WARMUP: usize = 3;
    const SAMPLES: usize = 21;
    const ITERATIONS: usize = 16;
    println!("# code_revision={revision}");
    println!("# reference_revision=6a2d459b32275b3742e56e16bceb638cba995ff3");
    println!("# warmup={WARMUP},samples={SAMPLES},iterations={ITERATIONS}");
    println!("# backend=NullRenderer,delta=0,unit=total_nanoseconds_per_sample");
    println!("# raw,mode,passes,entities,sample,iterations,total_ns");
    println!("# summary,mode,passes,entities,iterations,median_total_ns,p95_total_ns");
    for mode in ["hot", "replaced"] {
        for passes in [3, 32, 256, 2048] {
            for entities in [0, 512] {
                let mut engine = Engine::new();
                for index in 0..entities {
                    engine
                        .world_mut()
                        .try_spawn(Transform::from_translation(Vec3::new(
                            index as f32,
                            0.0,
                            0.0,
                        )))?;
                }
                let (initial, mut expected) = graph(passes, 0)?;
                *engine.render_graph_mut() = initial;
                let mut samples = Vec::with_capacity(SAMPLES);
                let mut expected_frame = 0;
                for sample in 0..WARMUP + SAMPLES {
                    let mut elapsed = 0;
                    for iteration in 0..ITERATIONS {
                        if mode == "replaced" {
                            // Same topology version but different names: construction/replacement
                            // is outside timing; compile and derived-name refresh remain inside.
                            let (replacement, names) =
                                graph(passes, sample * ITERATIONS + iteration)?;
                            *engine.render_graph_mut() = replacement;
                            expected = names;
                        }
                        let start = Instant::now();
                        let result = black_box(&mut engine).tick(black_box(0.0));
                        elapsed += start.elapsed().as_nanos();
                        let report = result?;
                        expected_frame += 1;
                        assert_eq!(report.frame, expected_frame);
                        assert_eq!(engine.last_render_passes(), expected);
                        assert_eq!(engine.last_frame_stats().submitted_commands, entities);
                    }
                    if sample >= WARMUP {
                        let sample_id = sample - WARMUP;
                        println!(
                            "raw,{mode},{passes},{entities},{sample_id},{ITERATIONS},{elapsed}"
                        );
                        samples.push(elapsed);
                    }
                }
                samples.sort_unstable();
                let median = samples[SAMPLES / 2];
                let p95 = samples[(SAMPLES * 95).div_ceil(100) - 1];
                println!("summary,{mode},{passes},{entities},{ITERATIONS},{median},{p95}");
            }
        }
    }
    Ok(())
}
