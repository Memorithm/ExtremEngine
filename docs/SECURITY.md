# ExtremEngine Security Boundaries & Hardening

ExtremEngine treats asset paths, serialized scenes, animation data and runtime numeric inputs as potentially malformed. These controls reduce specific risks; they are not a claim that the engine is universally secure.

## Current controls

1. **Asset paths fail closed** — `AssetKey::new` / `normalize_path` reject absolute paths, UNC paths, Windows drive prefixes, NUL bytes, excessive length and any `..` that would escape the virtual root. In-root normalization such as `a/../b` remains permitted. Invalid paths are rejected before loaders execute.
2. **Hash collisions do not define identity** — `Assets` retains the canonical `AssetKey` and rejects two distinct keys that map to the same 64-bit `AssetId` instead of silently overwriting an entry.
3. **Generational entity lifetime** — stale handles fail lookup, and a slot is permanently retired before its generation counter could wrap and make an ancient handle valid again. Fallible spawn APIs exist for resource-sensitive boundaries.
4. **Hierarchy corruption is bounded** — `set_parent`, `validate_hierarchy`, `propagate_transforms` and `despawn_recursive` use explicit cycle/visited handling. Validation checks both `Parent -> Children` and `Children -> Parent` directions and rejects duplicate child edges.
5. **Scene validation** — RON scene documents are version checked and bounded by node/depth limits; transforms must contain finite, non-degenerate rotation data before instantiation.
6. **Animation validation** — clip duration/times/values, strict keyframe ordering, quaternion normalization, track joint IDs, pose sizes and runtime delta/playback values are checked rather than silently ignored.
7. **Numerical fail-closed behavior** — science and minimal physics paths reject non-finite inputs/derivatives/state before committing invalid output.
8. **GPU surface validation** — surface format/alpha/present capability arrays are checked without unchecked indexing. WGPU acquisition outcomes (`Success`, `Suboptimal`, `Timeout`, `Occluded`, `Outdated`, `Lost`, `Validation`) are represented explicitly; recoverable loss is reconfigured rather than assumed successful.
9. **Supply chain** — `Cargo.lock` is committed, CI uses `--locked`, `cargo-deny` is retained, Actions are pinned, and the workspace lint forbids unsafe code.

## Deliberate non-capabilities

- No arbitrary shell execution is provided by editor/runtime APIs.
- No arbitrary network telemetry is introduced.
- The built-in WGPU validation shader is engine-owned; the engine does not yet expose automatic execution of untrusted shader source.
- A passing CPU/headless CI does not prove driver/GPU correctness on every platform.
- EEFP/VPAE are experimental contracts, not authorization to weaken hard invariants for frame-rate objectives.
