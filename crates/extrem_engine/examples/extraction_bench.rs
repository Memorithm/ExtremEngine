//! EE-PERF-03: paired command extraction, not a GPU or complete-frame benchmark.
use extrem_ecs::Entity;
use extrem_engine::RenderExtractor;
use extrem_math::{Transform, Vec3};
use extrem_render::RenderCommand;
use extrem_scene::GlobalTransform;
use std::hint::black_box;
use std::time::Instant;

#[path = "../tests/support/legacy_extraction.rs"]
mod legacy;
#[path = "../tests/support/extraction_fixture.rs"]
mod support;
use support::{Capture, Profile, fingerprint, fixture};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let revision = std::env::var("EXTREM_BENCH_REVISION")?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "EXTREM_BENCH_REVISION must be the checked-out 40-digit Git SHA",
        )
        .into());
    }
    const WARMUP: usize = 3;
    const SAMPLES: usize = 21;
    println!("# code_revision={revision}");
    println!("# legacy_revision=9fe9b2a13ec4b4a0df19e8f3ddcafdbeff06a0da");
    println!("# warmup={WARMUP},samples={SAMPLES},unit=nanoseconds");
    println!("# capture=preallocated,parity=all_command_bits_after_each_sample");
    println!("# command_bytes={}", std::mem::size_of::<RenderCommand>());
    println!(
        "# legacy_entry_bytes={}",
        std::mem::size_of::<(Entity, RenderCommand)>()
    );
    println!(
        "# scratch_entry_bytes={}",
        std::mem::size_of::<(Entity, Vec3)>()
    );
    println!("# raw,profile,nodes,implementation,sample,ns");
    println!("# summary,profile,nodes,implementation,median_ns,p95_ns");

    for profile in Profile::ALL {
        for nodes in [32, 512, 2048, 20_000] {
            let (mut world, ids) = fixture(profile, nodes)?;
            let mut capture = Capture {
                commands: Vec::with_capacity(nodes + 1),
            };
            let mut extractor = RenderExtractor::default();
            for _ in 0..WARMUP {
                capture.commands.clear();
                legacy::legacy_submit(black_box(&world), 1.0, &mut capture);
                capture.commands.clear();
                RenderExtractor::default().submit(black_box(&world), 1.0, &mut capture);
                capture.commands.clear();
                black_box(extractor.submit(black_box(&world), 1.0, &mut capture));
            }
            let mut measurements = [
                Vec::with_capacity(SAMPLES),
                Vec::with_capacity(SAMPLES),
                Vec::with_capacity(SAMPLES),
            ];
            for sample in 0..SAMPLES {
                // Change live inputs outside timing; no variant may reuse old commands.
                for &id in ids.iter().take(2) {
                    if let Some(local) = world.get_mut::<Transform>(id) {
                        local.translation.x = sample as f32 / 16.0;
                    }
                    if let Some(global) = world.get_mut::<GlobalTransform>(id) {
                        global.0.translation.x = -(sample as f32) / 8.0;
                    }
                }
                capture.commands.clear();
                legacy::legacy_submit(&world, 1.0, &mut capture);
                let expected = fingerprint(&capture.commands);
                for offset in 0..3 {
                    let implementation = (sample + offset) % 3;
                    capture.commands.clear();
                    let start = Instant::now();
                    match implementation {
                        0 => legacy::legacy_submit(black_box(&world), 1.0, &mut capture),
                        1 => {
                            black_box(RenderExtractor::default().submit(
                                black_box(&world),
                                1.0,
                                &mut capture,
                            ));
                        }
                        _ => {
                            black_box(extractor.submit(black_box(&world), 1.0, &mut capture));
                        }
                    }
                    let elapsed = start.elapsed().as_nanos();
                    assert_eq!(fingerprint(black_box(&capture.commands)), expected);
                    measurements[implementation].push(elapsed);
                }
            }
            for (name, mut samples) in ["legacy", "fresh", "reused"].into_iter().zip(measurements) {
                for (sample, elapsed) in samples.iter().enumerate() {
                    println!("raw,{},{nodes},{name},{sample},{elapsed}", profile.name());
                }
                samples.sort_unstable();
                let median = samples[SAMPLES / 2];
                let p95 = samples[(SAMPLES * 95).div_ceil(100) - 1];
                println!("summary,{},{nodes},{name},{median},{p95}", profile.name());
            }
        }
    }
    Ok(())
}
