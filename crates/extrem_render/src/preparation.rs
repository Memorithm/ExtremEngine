//! Derived pass-name snapshots owned by the renderer boundary, not by a version-only cache.
use crate::{CompiledRenderGraph, RenderGraph, RenderGraphError};
use std::sync::Arc;

/// Work performed by one successful preparation, not allocator or timing telemetry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderPlanPreparationStats {
    /// Passes in the prepared execution order.
    pub passes: usize,
    /// Whether this call rebuilt the name snapshot.
    pub refreshed_names: bool,
    /// UTF-8 name payload copied by this call; excludes headers/capacity/Arc storage.
    pub copied_name_bytes: usize,
}

/// Retains the last successful plan and its ordered pass names.
///
/// Reuse is based on a live Arc allocation identity, not topology version, length,
/// raw address, or graph location. An old plan is retained during comparison, so
/// replacing a graph cannot reuse an allocator address while that snapshot lives.
/// All actual graph edits invalidate its plan; no-op edits keep the same snapshot.
///
/// # Examples
/// ```
/// use extrem_render::{RenderGraph, RenderGraphError, RenderPlanPreparation};
/// let mut graph = RenderGraph::new();
/// graph.add_pass("clear");
/// let mut preparation = RenderPlanPreparation::default();
/// assert!(preparation.prepare(&mut graph)?.refreshed_names);
/// assert!(!preparation.prepare(&mut graph)?.refreshed_names);
/// assert_eq!(preparation.pass_names(), ["clear"]);
/// # Ok::<(), RenderGraphError>(())
/// ```
#[derive(Clone, Debug, Default)]
pub struct RenderPlanPreparation {
    plan: Option<Arc<CompiledRenderGraph>>,
    names: Vec<String>,
}

impl RenderPlanPreparation {
    /// Validates the graph and refreshes names only when the compiled snapshot changes.
    ///
    /// # Errors
    /// Propagates graph compilation or missing-name errors before replacing this
    /// object's last successful snapshot. It never treats an old plan as a fallback
    /// for an invalid current graph. Allocation failures/panics are not intercepted.
    pub fn prepare(
        &mut self,
        graph: &mut RenderGraph,
    ) -> Result<RenderPlanPreparationStats, RenderGraphError> {
        let compiled = graph.compile_shared()?;
        let refreshed_names = self.plan.as_ref().is_none_or(|old| !Arc::ptr_eq(old, &compiled));
        let mut stats = RenderPlanPreparationStats {
            passes: compiled.execution_order.len(),
            refreshed_names,
            copied_name_bytes: 0,
        };
        if refreshed_names {
            let names: Vec<String> = compiled
                .execution_order
                .iter()
                .map(|pass| {
                    graph
                        .pass_name(*pass)
                        .ok_or(RenderGraphError::MissingPass(*pass))
                        .map(str::to_owned)
                })
                .collect::<Result<_, _>>()?;
            stats.copied_name_bytes = names.iter().map(String::len).sum();
            self.names = names;
            self.plan = Some(compiled);
        }
        Ok(stats)
    }

    /// Names from the most recent successful preparation, initially empty.
    pub fn pass_names(&self) -> &[String] {
        &self.names
    }

    /// Releases this object's snapshot. Other Arc owners can keep its plan alive.
    ///
    /// This intentionally clears standalone diagnostics; it does not promise an
    /// operating-system RSS reduction or release other graph/backend allocations.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
