# Editor transaction contract

`extrem_editor` operates on a caller-owned ECS `World`; it does not start a GUI,
network service, filesystem writer, or scripting runtime. Keep an `EditorState`
with the same world. Call `clear_history()` when replacing that world or when
external edits should invalidate previously recorded state.

## Exact replay and failure atomicity

`apply`, `undo`, and `redo` validate before modifying the requested field. A
returned `Err` leaves world state, selection and both history stacks unchanged.
A failed undo/redo retains the same record in the same position so the caller
can repair a missing component and retry. Panic/OOM recovery is not promised.

`Translate` validates the old position, delta and computed result for finiteness.
Its `CommandRecord` stores two absolute `SetTranslation` commands, not `+delta`
and `-delta`: floating-point subtraction cannot in general reconstruct the
previous bit pattern. Replay preserves saved position bits, including signed zero,
without restoring unrelated rotation/scale fields. This is field-level history,
not a snapshot of arbitrary ECS components.

`SetVisible` records whether `Visibility` originally existed. Its inverse uses
`RemoveVisibility` when needed; an absent component is not replaced by
`Visibility(true)`. Both new variants are public commands. Downstream exhaustive
matches on `EditorCommand` must handle them.

`Rename` requires an existing `Name`; missing names and missing entities produce
different errors. New edits invalidate redo only after they succeed. Selection
changes neither create records nor discard redo.

## Bounded retention

```rust
use extrem_editor::{EditorLimits, EditorState};

let mut editor = EditorState::with_limits(EditorLimits {
    max_history_entries: 128,
    max_name_bytes: 4096,
});
assert_eq!((editor.undo_len(), editor.redo_len()), (0, 0));
editor.clear_history();
```

Defaults retain at most **256 combined undo/redo records**. Undo/redo moves a
record rather than increasing that total. When the limit is reached, a new edit
evicts the oldest undo record. A zero record limit disables history, not editing.
No allocation proportional to the configured entry limit happens at construction.

Both the previous and next name must fit `max_name_bytes` (4096 by default),
measured in UTF-8 bytes before cloning history strings. This bounds retained name
payloads as well as record count. It is not a global ECS memory quota; caller-owned
strings and arbitrary components remain the caller's responsibility.

## Deletion policy

`Delete(entity)` now means deletion of that entity **and its reachable subtree**.
Before any mutation, `validate_hierarchy` checks the current world's bidirectional
links. A corrupt edge must not authorize destruction of an unrelated entity.
The existing `extrem_scene::despawn_recursive` implementation performs deletion
and detaches the subtree from its surviving parent. Selection is cleared only
following successful deletion, including selection of a deleted descendant.

Successful deletion clears both histories: it is an explicit irreversible barrier.
Undoing deletion of arbitrary type-erased components is NOT implemented. This
change does not invent a partial restore or reuse stale entity generations.

Whole-world validation favors correctness over deletion throughput. It also means
an unrelated corrupt hierarchy blocks deletion until repaired. No new complexity
or performance claim is made for the existing validator. Independently owned
`Scene::roots` lists are not registered with `EditorState`; their owner must keep
those lists synchronized after deleting a root.

## Validation

```bash
cargo test -p extrem_editor --test history_regressions --locked
cargo test -p extrem_editor --test history_contract --locked
cargo test --workspace --all-targets --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

`history_regressions` contains ten executable reproductions written before the
fix. `history_contract` adds nine tests for bounds, branching, exact mixed-command
replay, UTF-8 names, stale generations, failed rename/deletion and visibility.
The two existing unit tests remain. The native CI jobs additionally execute the
editor tests on Windows/macOS; Linux MSRV/stable and quality/security gates remain.

## Cross-project reuse review

The transaction contract can inform a future experiment editor, but it is not a
reason to copy this ECS-specific implementation into a research core. TDI's inspected
root manifest declares Rust 1.85, while ExtremEngine's current CI covers MSRV 1.87;
its `tdi-operator` crate is for tridiagonal research operators, not an editor.
No TDI dependency or source change is made by this increment. A shared journal
should be extracted only with a concrete consumer and compatibility tests.
