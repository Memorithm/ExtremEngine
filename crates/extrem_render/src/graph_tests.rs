//! Small-graph differential oracle and deep-graph safety tests.
use super::{RenderGraph, RenderGraphError, RenderPassId};

// Frozen recursive visit semantics from df2f03c781b1a7a03de69f7ef1ca74ac522e350f.
// Used only on four-node fixtures, never on the deep-graph tests.
fn legacy_visit(
    index: usize,
    graph: &RenderGraph,
    states: &mut [u8],
    order: &mut Vec<RenderPassId>,
) -> Result<(), RenderGraphError> {
    match states[index] {
        1 => return Err(RenderGraphError::Cycle(RenderPassId(index))),
        2 => return Ok(()),
        _ => {}
    }
    states[index] = 1;
    for dependency in &graph.passes[index].dependencies {
        if graph.passes.get(dependency.0).is_none() {
            return Err(RenderGraphError::MissingPass(*dependency));
        }
        legacy_visit(dependency.0, graph, states, order)?;
    }
    states[index] = 2;
    order.push(RenderPassId(index));
    Ok(())
}

fn legacy_order(graph: &RenderGraph) -> Result<Vec<RenderPassId>, RenderGraphError> {
    let mut states = vec![0; graph.passes.len()];
    let mut order = Vec::new();
    for index in 0..graph.passes.len() {
        legacy_visit(index, graph, &mut states, &mut order)?;
    }
    Ok(order)
}

#[test]
fn all_65536_four_node_graphs_preserve_order_and_first_error() {
    for mask in 0..65_536_u32 {
        let mut graph = RenderGraph::new();
        let ids: Vec<_> = (0..4)
            .map(|index| graph.add_pass(index.to_string()))
            .collect();
        for (from, &pass) in ids.iter().enumerate() {
            for (to, &dependency) in ids.iter().enumerate() {
                if mask & (1 << (from * 4 + to)) != 0 {
                    graph.add_dependency(pass, dependency).unwrap();
                }
            }
        }
        let expected = legacy_order(&graph);
        let result = graph.compile();
        assert_eq!(
            result.as_ref().map(|p| &p.execution_order),
            expected.as_ref(),
            "{mask}"
        );
        if result.is_err() {
            assert!(graph.cached_plan().is_none());
        }
        assert_eq!(graph.compile(), result, "retry must be stable: {mask}");
    }
}

#[test]
fn deep_chain_and_cycle_use_heap_scratch_not_recursive_stack() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let mut graph = RenderGraph::new();
            let ids: Vec<_> = (0..50_000).map(|_| graph.add_pass("pass")).collect();
            // Descending evaluation requires one active DFS frame per vertex.
            for pair in ids.windows(2) {
                graph.add_dependency(pair[0], pair[1]).unwrap();
            }
            let expected: Vec<_> = ids.iter().rev().copied().collect();
            assert_eq!(graph.compile().unwrap().execution_order, expected);
            graph.add_dependency(ids[49_999], ids[0]).unwrap();
            assert_eq!(graph.compile(), Err(RenderGraphError::Cycle(ids[0])));
            assert!(graph.cached_plan().is_none());
            assert!(graph.remove_dependency(ids[49_999], ids[0]).unwrap());
            assert_eq!(graph.compile().unwrap().execution_order, expected);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn removal_changes_version_only_for_an_existing_edge() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("a");
    let b = graph.add_pass("b");
    graph.add_dependency(a, b).unwrap();
    let plan = graph.compile().unwrap();
    assert!(!graph.remove_dependency(b, a).unwrap());
    assert_eq!(graph.version(), plan.version);
    assert_eq!(graph.cached_plan(), Some(&plan));
    assert!(graph.remove_dependency(a, b).unwrap());
    assert_eq!(graph.version(), plan.version.wrapping_add(1));
    assert!(graph.cached_plan().is_none());
    assert_eq!(graph.compile().unwrap().execution_order, vec![a, b]);
}

#[test]
fn missing_ids_do_not_mutate_dependencies_or_cache() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("a");
    let b = graph.add_pass("b");
    graph.add_dependency(a, b).unwrap();
    let plan = graph.compile().unwrap();
    let missing = RenderPassId(usize::MAX);
    for (pass, dependency) in [(missing, a), (a, missing)] {
        assert_eq!(
            graph.remove_dependency(pass, dependency),
            Err(RenderGraphError::MissingPass(missing))
        );
        assert_eq!(graph.cached_plan(), Some(&plan));
        assert_eq!(graph.version(), plan.version);
    }
    assert_eq!(graph.passes[a.0].dependencies, vec![b]);
}

#[test]
fn corrupted_stored_dependency_returns_error_without_partial_cache() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("a");
    graph.passes[a.0]
        .dependencies
        .push(RenderPassId(usize::MAX));
    assert_eq!(
        graph.compile(),
        Err(RenderGraphError::MissingPass(RenderPassId(usize::MAX)))
    );
    assert!(graph.cached_plan().is_none());
    graph.passes[a.0].dependencies.clear();
    assert_eq!(graph.compile().unwrap().execution_order, vec![a]);
}

#[test]
fn clone_and_version_wrap_do_not_reuse_an_invalid_plan() {
    let mut original = RenderGraph::new();
    let a = original.add_pass("a");
    let plan = original.compile().unwrap();
    let mut edited = original.clone();
    edited.version = u64::MAX;
    edited.add_dependency(a, a).unwrap();
    assert_eq!(edited.version(), 0);
    assert!(edited.cached_plan().is_none());
    assert_eq!(edited.compile(), Err(RenderGraphError::Cycle(a)));
    assert_eq!(original.compile(), Ok(plan));
    edited.remove_dependency(a, a).unwrap();
    assert_eq!(edited.compile().unwrap().execution_order, vec![a]);
}

#[test]
fn empty_graph_compiles_and_caches_an_empty_plan() {
    let mut graph = RenderGraph::new();
    let plan = graph.compile().unwrap();
    assert!(plan.execution_order.is_empty());
    assert_eq!(graph.cached_plan(), Some(&plan));
}
