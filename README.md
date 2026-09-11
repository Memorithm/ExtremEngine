# ExtremEngine

ExtremEngine est un moteur de jeu Rust modulaire **en construction**. Le dépôt privilégie des contrats explicites de sûreté, de validation numérique et d'ordre d'exécution là où le résultat dépend de l'ordre. Il ne revendique pas encore une sûreté universelle, un déterminisme inter-plateforme complet ni un niveau de production AAA.

> Branche de développement actuelle : **`main`**.
> La branche par défaut GitHub `agent/initial-engine` est en retard (LOD/DRS absents). Cloner avec :
> `git clone --branch main https://github.com/Memorithm/ExtremEngine.git`

## Modules du workspace

- `extrem_ecs` : entités générationnelles, composants, ressources et monde. Les slots sont retirés avant wrap de génération. `remove_resource` est disponible.
- `extrem_math` : `Vec3`, `Quat`, `Mat4` et `Transform`; produit de Hamilton, composition TRS et projection WebGPU `z \u2208 [0,1]` sont testés. Les projections dégénérées sont assainies.
- `extrem_scene` : hiérarchie parent/enfant, validation bidirectionnelle, traversal bornée contre les cycles corrompus, caméras, documents RON validés et `Scene::prune_roots`.
- `extrem_editor` : transactions `CommandRecord`, undo/redo pour rename/translate/visibility, inspection. `Delete` n'est pas réversible (identité générationnelle).
- `extrem_assets` : clés de chemins virtuels validées en mode fail-closed, détection de collisions et handles typés.
- `extrem_app` : stages, fixed timestep borné, signalement de dette de simulation abandonnée, budget de frame, LOD déterministe et politique DRS.
- `extrem_input` : clavier, souris et transitions de boutons.
- `extrem_window` : boucle `winit`; les événements natifs sont effectivement injectés dans l'état `Input` avant chaque callback de frame.
- `extrem_gpu` : contexte `wgpu`, surface de fenêtre avec gestion explicite des états d'acquisition et `WgpuPresenter` qui valide un chemin réel shader → render pass → draw → present.
- `extrem_render` : contrat de backend, renderer nul/CPU et render graph persistant avec plan topologique mis en cache.
- `extrem_web` : détection et validation des capacités d'exécution Web/WebGPU en contexte sécurisé.
- `extrem_animation` : squelette, clips validés, sampling, nlerp/slerp, blending de poses, palette LBS et contrats transactionnels EEFP/VPAE expérimentaux.
- `extrem_physics` : **solveur de référence minimal** (gravité + sol + box), avec validation des données. Ce n'est pas encore un solveur rigid-body général.
- `extrem_science` : Euler/RK4 avec validation numérique et workspace RK4 réutilisable.
- `extrem_audio` : contrat de commandes/backend audio et backend nul; sortie audio de production encore à implémenter.
- `extrem_engine` : façade haut niveau, extraction déterminisée (entités cachées exclues), render graph persistant, mesure CPU/DRS et adaptateur `WgpuRenderer`.

## Validation

```bash
git checkout main
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

La CI vérifie également le MSRV Rust 1.87 et compile le workspace sur Linux, Windows et macOS. Une compilation réussie ne constitue pas à elle seule une validation matérielle du rendu WGPU; la présentation sur GPU réel doit être qualifiée séparément.

Licence : MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

## Documentation

- `docs/ARCHITECTURE.md`
- `docs/TRANSFORMS.md`
- `docs/ANIMATION.md`
- `docs/SECURITY.md`
