# ExtremEngine licence and provenance inventory

Status: engineering inventory for audit findings `BF-EE-05` and `BF-EE-06`; not legal advice and not a relicensing instrument.

## Pinned scope

This inventory was prepared from the authoritative GitHub default branch `agent/initial-engine` at base commit `1eb9b60a21944d980088edbc90f5e6175bb8800e` on 2026-10-11. The complete recursive tree was inspected; no `vendor/` directory, standalone binary asset, copied specification, or independent file-level SPDX/copyright header was found in that tree. Re-run the classification when one is introduced.

## Distributed surfaces and obligations

| Surface | Classification | Licence / obligation | Evidence |
|---|---|---|---|
| Workspace crates under `crates/extrem_*` | Repository first-party surface | `MIT OR Apache-2.0`; recipients may choose either complete root licence | workspace `Cargo.toml`, `LICENSE-MIT`, `LICENSE-APACHE` |
| Root documentation, workflows and repository-owned tests/examples | Repository first-party surface unless a file says otherwise | same repository-level choice; preserve notices and history | recursive tree at the pinned base |
| Cargo registry dependencies | Third-party, not relicensed by ExtremEngine | each package's own Cargo metadata/licence files | `Cargo.lock`; `cargo deny check licenses sources` |
| Optional `elastic-quality` graph | Third-party/Memorithm integration | ElasticXxx crates are `PolyForm-Noncommercial-1.0.0`; enabling the feature adds those terms | exact git source in lock/manifest; scoped `deny.toml` exceptions; `docs/ELASTIC_ADAPTIVE_QUALITY.md` |
| Generated benchmark/render output | Runtime output, not distributed source in this tree | no blanket ownership or licence assertion is made | examples/workflows generate output; tree contains no captured output artefacts |

The Apache text is now materialized at the repository root. This fixes the packaging mismatch where Cargo advertised `MIT OR Apache-2.0` but only the MIT text was distributed. It does not modify the existing MIT grant and does not assert that historical recipients lost any permission.

## Copyright and history boundary

The visible history includes commits authored under the identities `Memorithm` and `MEMOPERF` / GitHub account `CHECKUPAUTO`. Those author records preserve attribution but do not, by themselves, prove ownership, assignment, or authority to relicense every historical contribution. The root MIT notice names `Memorithm`; this inventory does not expand that statement.

Accordingly:

- the present `MIT OR Apache-2.0` metadata and both matching root texts remain intact;
- third-party dependency and optional ElasticXxx terms remain separate;
- issue #33 continues to block a future PolyForm convergence until rightsholder/contributor authority is established;
- any future transition must preserve historical grants and third-party notices rather than replacing them by automation.

This is the terminal classification for the current distributed surface: licence files and metadata are coherent, feature-specific obligations are explicit, and the unresolved rights question is isolated to a future relicensing proposal rather than left ambiguous in current packaging.

## Bounded advisory exception

RustSec `RUSTSEC-2026-0192` is an informational *unmaintained* notice for `ttf-parser`, with no patched version and `skrifa` named as an alternative. The current graph reaches it only through `winit -> sctk-adwaita -> ab_glyph` for client-side window decoration; ExtremEngine has no direct dependency or public caller-supplied font parsing API.

Issue #46 owns removal. The exception is valid only through 2026-12-31. `scripts/check-advisory-waivers.sh` fails CI after that date, if owner/issue/expiry metadata disappears, if `ttf-parser` becomes direct, or if the expected dependency-path proof changes. A renewal requires a new primary-advisory review and a new explicit expiry.
