use extrem_render::{RenderGraph, RenderGraphError, RenderPlanPreparation};
use std::sync::Arc;

#[test]
fn shared_hits_preserve_identity_and_owned_plans_are_independent() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("a");
    let b = graph.add_pass("b");
    graph.add_dependency(a, b).unwrap();
    let first = graph.compile_shared().unwrap();
    assert_eq!(first.execution_order, vec![b, a]);
    assert!(Arc::ptr_eq(&first, &graph.compile_shared().unwrap()));
    let mut owned = graph.compile().unwrap();
    owned.execution_order.clear();
    assert_eq!(graph.compile().unwrap(), *first);
    let mut external = graph.compile_shared().unwrap();
    Arc::make_mut(&mut external).execution_order.clear();
    assert_eq!(graph.cached_plan(), Some(first.as_ref()));
    assert!(Arc::ptr_eq(&first, &graph.compile_shared().unwrap()));
}

#[test]
fn noop_and_rejected_edits_keep_the_same_plan() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("a");
    let b = graph.add_pass("b");
    graph.add_dependency(a, b).unwrap();
    let first = graph.compile_shared().unwrap();
    graph.add_dependency(a, b).unwrap();
    assert!(!graph.remove_dependency(b, a).unwrap());
    let mut other = RenderGraph::new();
    other.add_pass("x");
    other.add_pass("y");
    let outside = other.add_pass("z");
    assert_eq!(graph.add_dependency(a, outside), Err(RenderGraphError::MissingPass(outside)));
    assert_eq!(graph.remove_dependency(a, outside), Err(RenderGraphError::MissingPass(outside)));
    assert!(Arc::ptr_eq(&first, &graph.compile_shared().unwrap()));
}

#[test]
fn edits_invalidate_even_when_old_snapshots_remain_alive() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("a");
    let old = graph.compile_shared().unwrap();
    graph.add_dependency(a, a).unwrap();
    assert_eq!(graph.compile_shared(), Err(RenderGraphError::Cycle(a)));
    assert!(graph.cached_plan().is_none());
    assert_eq!(old.execution_order, vec![a]);
    graph.remove_dependency(a, a).unwrap();
    let repaired = graph.compile_shared().unwrap();
    assert!(!Arc::ptr_eq(&old, &repaired));
    assert_eq!(old.execution_order, repaired.execution_order);
    graph.add_pass("b");
    assert!(!Arc::ptr_eq(&repaired, &graph.compile_shared().unwrap()));
}

#[test]
fn cloning_then_editing_graph_does_not_change_original_or_snapshot() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("a");
    let original = graph.compile_shared().unwrap();
    let mut clone = graph.clone();
    assert!(Arc::ptr_eq(&original, &clone.compile_shared().unwrap()));
    clone.add_dependency(a, a).unwrap();
    assert!(clone.compile_shared().is_err());
    assert_eq!(graph.compile().unwrap(), *original);
    assert!(Arc::ptr_eq(&original, &graph.compile_shared().unwrap()));
}

#[test]
fn replacing_same_version_graph_refreshes_names_not_only_topology() {
    let mut graph = RenderGraph::new();
    graph.add_pass("before");
    let mut prepared = RenderPlanPreparation::default();
    assert!(prepared.prepare(&mut graph).unwrap().refreshed_names);
    let mut replacement = RenderGraph::new();
    replacement.add_pass("after");
    assert_eq!(replacement.version(), graph.version());
    graph = replacement;
    assert!(prepared.prepare(&mut graph).unwrap().refreshed_names);
    assert_eq!(prepared.pass_names(), ["after"]);
    let stable = prepared.prepare(&mut graph).unwrap();
    assert!(!stable.refreshed_names);
    assert_eq!(stable.copied_name_bytes, 0);
}

#[test]
fn preparation_preserves_names_on_failure_then_recovers() {
    let mut graph = RenderGraph::new();
    let a = graph.add_pass("é");
    let mut prepared = RenderPlanPreparation::default();
    assert_eq!(prepared.prepare(&mut graph).unwrap().copied_name_bytes, 2);
    let pointer = prepared.pass_names()[0].as_ptr();
    graph.add_dependency(a, a).unwrap();
    for _ in 0..3 {
        assert_eq!(prepared.prepare(&mut graph), Err(RenderGraphError::Cycle(a)));
        assert_eq!(prepared.pass_names(), ["é"]);
        assert_eq!(prepared.pass_names()[0].as_ptr(), pointer);
    }
    graph.remove_dependency(a, a).unwrap();
    assert!(prepared.prepare(&mut graph).unwrap().refreshed_names);
    assert_eq!(prepared.pass_names(), ["é"]);
}

#[test]
fn empty_duplicate_and_unicode_names_have_exact_order_and_byte_counts() {
    let mut graph = RenderGraph::new();
    let mut prepared = RenderPlanPreparation::default();
    assert_eq!(prepared.prepare(&mut graph).unwrap().passes, 0);
    for name in ["", "é", "é"] {
        graph.add_pass(name);
    }
    let stats = prepared.prepare(&mut graph).unwrap();
    assert_eq!((stats.passes, stats.copied_name_bytes), (3, 4));
    assert_eq!(prepared.pass_names(), ["", "é", "é"]);
    let pointer = prepared.pass_names().as_ptr();
    assert!(!prepared.prepare(&mut graph).unwrap().refreshed_names);
    assert_eq!(pointer, prepared.pass_names().as_ptr());
}

#[test]
fn snapshot_lifetime_and_clear_release_owned_references() {
    let mut graph = RenderGraph::new();
    graph.add_pass("a");
    let mut prepared = RenderPlanPreparation::default();
    prepared.prepare(&mut graph).unwrap();
    let plan = graph.compile_shared().unwrap();
    let weak = Arc::downgrade(&plan);
    drop(plan);
    drop(graph);
    assert!(weak.upgrade().is_some());
    prepared.clear();
    assert!(prepared.pass_names().is_empty());
    assert!(weak.upgrade().is_none());
}
