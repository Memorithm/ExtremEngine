//! Paired CPU microbenchmark. Run in release mode with EXTREM_BENCH_REVISION set.
use extrem_ecs::{World, WorldError};
use extrem_scene::{Children, HierarchyError, HierarchyValidator, Parent, validate_hierarchy};
use std::hint::black_box;
use std::time::Instant;

#[path = "../tests/support/legacy_hierarchy.rs"]
mod legacy;

#[derive(Clone, Copy)]
enum Shape {
    Chain,
    Wide,
    Balanced,
}

impl Shape {
    fn name(self) -> &'static str {
        match self {
            Self::Chain => "chain",
            Self::Wide => "wide",
            Self::Balanced => "balanced",
        }
    }
}

fn fixture(shape: Shape, nodes: usize) -> Result<World, WorldError> {
    let mut world = World::new();
    let mut entities = Vec::with_capacity(nodes);
    for _ in 0..nodes {
        entities.push(world.try_spawn_empty()?);
    }
    let mut children = vec![Vec::new(); nodes];
    for index in 1..nodes {
        let parent = match shape {
            Shape::Chain => index - 1,
            Shape::Wide => 0,
            Shape::Balanced => (index - 1) / 2,
        };
        world.insert(entities[index], Parent(entities[parent]))?;
        children[parent].push(entities[index]);
    }
    for (entity, children) in entities.into_iter().zip(children) {
        world.insert(entity, Children(children))?;
    }
    Ok(world)
}

fn timed<T>(run: impl FnOnce() -> Result<T, HierarchyError>) -> Result<u128, HierarchyError> {
    let start = Instant::now();
    let result = run()?;
    black_box(result);
    Ok(start.elapsed().as_nanos())
}

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
    const SAMPLES: usize = 11;
    println!("# code_revision={revision}");
    println!("# legacy_revision=9dc2a79020f3bd000e985db807cbc474942e52d5");
    println!("# warmup={WARMUP},samples={SAMPLES},unit=nanoseconds");
    println!("# raw,shape,nodes,implementation,sample,ns");
    println!("# summary,shape,nodes,implementation,median_ns,p95_ns");

    for shape in [Shape::Chain, Shape::Wide, Shape::Balanced] {
        for nodes in [32, 512, 2048] {
            let world = fixture(shape, nodes)?;
            let mut validator = HierarchyValidator::default();
            for _ in 0..WARMUP {
                legacy::legacy_validate_hierarchy(black_box(&world))?;
                validate_hierarchy(black_box(&world))?;
                black_box(validator.validate(black_box(&world))?);
            }
            let mut measurements = [
                Vec::with_capacity(SAMPLES),
                Vec::with_capacity(SAMPLES),
                Vec::with_capacity(SAMPLES),
            ];
            // Rotate execution order to reduce systematic first/last bias.
            // Fixture construction, output and sorting are outside timed sections.
            for sample in 0..SAMPLES {
                for offset in 0..3 {
                    let implementation = (sample + offset) % 3;
                    let elapsed = match implementation {
                        0 => timed(|| legacy::legacy_validate_hierarchy(black_box(&world)))?,
                        1 => timed(|| validate_hierarchy(black_box(&world)))?,
                        _ => timed(|| validator.validate(black_box(&world)))?,
                    };
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
        }
    }
    Ok(())
}
