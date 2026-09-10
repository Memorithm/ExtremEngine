# ExtremEngine Security Boundaries & Hardening

ExtremEngine treats asset files, scene documents, and editor input as potentially untrusted data.

## Security Controls

1. **Path Traversal Protection**:
   `extrem_assets::normalize_path` strips drive letters, normalizes separators (`\` to `/`), resolves relative directory components (`.` and `..`), and prevents escaping asset roots.
2. **Hash Collision Safety**:
   `Assets::insert` and `Assets::load_with` verify path equivalence for 64-bit `AssetId` hashes, rejecting collisions rather than silently overwriting asset values.
3. **Generational Handles**:
   `Entity` IDs in `extrem_ecs` use index and generation counter. Stale handles from despawned entities fail lookups safely.
4. **Hierarchy Invariants**:
   `extrem_scene::set_parent` and `validate_hierarchy` prevent self-parenting and cycle creation. Transform propagation uses iterative visited checking to prevent stack overflows or infinite loops.
5. **Supply Chain Protection**:
   All dependencies are checked via `cargo deny` (`deny.toml`). `Cargo.lock` is committed and locked CI builds are enforced. Unsafe code is forbidden across the workspace (`unsafe_code = "forbid"`).
