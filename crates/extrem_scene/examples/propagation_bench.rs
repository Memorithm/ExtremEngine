//! EE-PERF-02: same-world paired propagation microbenchmark, not an FPS benchmark.
use extrem_math::{Transform, Vec3};
use extrem_scene::{GlobalTransform, TransformPropagator, propagate_transforms};
use std::hint::black_box;
use std::time::Instant;

#[path = "../tests/support/propagation_fixture.rs"]
mod fixture;
#[path = "../tests/support/legacy_propagation.rs"]
mod legacy;
use fixture::{Shape, fixture, snapshot};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let revision = std::env::var("EXTREM_BENCH_REVISION")?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "EXTREM_BENCH_REVISION must be the 40-digit checked-out Git SHA",
        )
        .into());
    }
    const WARMUP: usize = 3;
    const SAMPLES: usize = 21;
    println!("# code_revision={revision}");
    println!("# legacy_revision=a00b6e513162d9a82473dfa5fe041e6cda465756");
    println!("# warmup={WARMUP},samples={SAMPLES},unit=nanoseconds");
    println!("# globals=preallocated,parity=all_TRS_bits_after_each_sample");
    println!("# raw,shape,nodes,implementation,sample,ns");
    println!("# summary,shape,nodes,implementation,median_ns,p95_ns");
    println!("# scratch,shape,nodes,pending_capacity,visited_capacity");

    for shape in Shape::ALL {
        for nodes in [32, 512, 2048, 20_000] {
            let (mut world, ids) = fixture(shape, nodes)?;
            let mut scratch = TransformPropagator::default();
            for _ in 0..WARMUP {
                legacy::legacy_propagate_transforms(black_box(&mut world));
                propagate_transforms(black_box(&mut world));
                black_box(scratch.propagate(black_box(&mut world)));
            }
            let mut measurements = [
                Vec::with_capacity(SAMPLES),
                Vec::with_capacity(SAMPLES),
                Vec::with_capacity(SAMPLES),
            ];
            for sample in 0..SAMPLES {
                // Change an input each sample. Reference production math, not cached globals.
                world
                    .get_mut::<Transform>(ids[0])
                    .expect("fixture root")
                    .translation
                    .x = sample as f32 / 128.0;
                legacy::legacy_propagate_transforms(&mut world);
                let expected = snapshot(&world);
                for offset in 0..3 {
                    let implementation = (sample + offset) % 3;
                    // Reset outputs outside timing; every variant must recompute all outputs.
                    for (_, global) in world.iter_mut::<GlobalTransform>() {
                        global.0 = Transform::from_translation(Vec3::new(-999.0, 5.0, 7.0));
                    }
                    let start = Instant::now();
                    match implementation {
                        0 => legacy::legacy_propagate_transforms(black_box(&mut world)),
                        1 => propagate_transforms(black_box(&mut world)),
                        _ => {
                            black_box(scratch.propagate(black_box(&mut world)));
                        }
                    }
                    let elapsed = start.elapsed().as_nanos();
                    assert_eq!(snapshot(black_box(&world)), expected, "propagation parity");
                    measurements[implementation].push(elapsed);
                }
            }
            for (name, mut samples) in ["legacy", "fresh", "reused"].into_iter().zip(measurements) {
                for (sample, elapsed) in samples.iter().enumerate() {
                    println!("raw,{},{nodes},{name},{sample},{elapsed}", shape.name());
                }
                samples.sort_unstable();
                let median = samples[samples.len() / 2];
                let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];
                println!("summary,{},{nodes},{name},{median},{p95}", shape.name());
            }
            let (pending, visited) = scratch.scratch_capacity();
            println!("scratch,{},{nodes},{pending},{visited}", shape.name());
        }
    }
    Ok(())
}
