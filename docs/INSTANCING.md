# Consecutive mesh instancing

ExtremEngine's indexed WGPU mesh path keeps ECS/entity order authoritative. It does not globally sort opaque draws by mesh because equal-depth fragments can make draw order observable with the current `Less` depth comparison.

The renderer therefore applies a conservative optimization: only a maximal consecutive run of logical draws that references the same uploaded `MeshData` is encoded as one `draw_indexed` call with an instance range. A sequence `A, A, A, B, A` becomes three encoded GPU draws (`A×3`, `B×1`, `A×1`), never two. Instance matrices and colors remain in their original slots.

`MeshFrameReport::draw_calls` remains the number of logical visible mesh instances for compatibility. `MeshFrameReport::encoded_draw_calls` reports the actual number of indexed draw commands emitted after this batching step. Neither value is a GPU-time or FPS measurement.

The optimization is intentionally narrow: it does not reorder entities, merge different geometries, change materials, alter depth state, introduce indirect draws, or claim hardware performance. It is useful immediately for scenes that spawn adjacent entities sharing one immutable geometry, including the existing cube example.

Validation consists of CPU tests for maximal runs and non-reordering plus the existing required WGPU pixel qualification, which exercises multiple instances sharing one geometry. Any future global batching/sorting must first define an explicit render-order/material contract and prove pixel equivalence for equal-depth cases.
