# ExtremEngine

ExtremEngine est un moteur de jeu Rust modulaire **en construction**. Le dépôt privilégie des contrats explicites de sûreté, de validation numérique et d'ordre d'exécution là où le résultat dépend de l'ordre. Il ne revendique pas encore une sûreté universelle, un déterminisme inter-plateforme complet ni un niveau de production AAA.

## Modules du workspace

- `extrem_ecs` : entités générationnelles, composants, ressources et monde. Les slots sont retirés avant wrap de génération.
- `extrem_math` : `Vec3`, `Quat`, `Mat4` et `Transform`; produit de Hamilton, composition TRS et projection WebGPU `z ∈ [0,1]` sont testés.
- `extrem_scene` : hiérarchie parent/enfant, validation bidirectionnelle sans reparcours quadratiques, propagation des transformations sans copie des listes d'enfants et espaces de travail réutilisables, caméras et documents RON validés.
- `extrem_editor` : historique borné, undo/redo exact et conservé sur erreur, validation des translations et suppression de sous-arbres validés (irréversible, avec remise à zéro de l'historique). Voir `docs/EDITOR.md`.
- `extrem_assets` : clés de chemins virtuels validées en mode fail-closed, détection de collisions et handles typés.
- `extrem_app` : stages, fixed timestep borné, signalement de dette de simulation abandonnée et assainissement du temps non fini.
- `extrem_input` : clavier, souris et transitions de boutons.
- `extrem_window` : boucle `winit`; les événements natifs sont effectivement injectés dans l'état `Input` avant chaque callback de frame.
- `extrem_gpu` : contexte `wgpu`, surface de fenêtre avec gestion explicite des états d'acquisition et `WgpuPresenter` qui valide un chemin réel shader → render pass → draw → present.
- `extrem_render` : contrat de backend, renderer nul/CPU et render graph persistant avec plan topologique mis en cache.
- `extrem_web` : détection et validation des capacités d'exécution Web/WebGPU en contexte sécurisé.
- `extrem_animation` : squelette, clips validés, sampling, nlerp/slerp, blending de poses, palette LBS et contrats transactionnels EEFP/VPAE expérimentaux.
- `extrem_physics` : **solveur de référence minimal** (gravité + sol + box), avec validation des données. Ce n'est pas encore un solveur rigid-body général.
- `extrem_science` : Euler/RK4 avec validation numérique et workspace RK4 réutilisable.
- `extrem_audio` : contrat de commandes/backend audio et backend nul; sortie audio de production encore à implémenter.
- `extrem_engine` : façade haut niveau, propagation réutilisable intégrée à PostUpdate avec statistiques, extraction déterminisée, render graph persistant et adaptateur `WgpuRenderer` vers le presenter GPU de validation.

## Programme de performance

`EE-PERF-01` porte sur le validateur hiérarchique. `EE-PERF-02` porte sur la propagation des transformations : suppression des copies des listes d'enfants, réemploi des buffers dans le moteur et comparaison bit à bit avec l'ancienne implémentation. Les deux séries conservent leur oracle historique et leur microbenchmark CPU avant/après sur plusieurs tailles et formes de scènes. Le workflow `Scene Performance` conserve les échantillons bruts, le SHA réellement exécuté et l'environnement dans ses artefacts. La méthode et les prochaines tranches sont décrites dans `docs/PERFORMANCE.md`.

Ces mesures CPU sur runners partagés ne constituent ni une qualification GPU ni une promesse de FPS. Les gains, les régressions éventuelles et leurs limites doivent être évalués à partir des sorties effectivement produites.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

La CI vérifie également le MSRV Rust 1.87 et compile le workspace sur Linux, Windows et macOS. Les jobs Windows/macOS exécutent aussi les tests transactionnels de l'éditeur. Une compilation réussie ne constitue pas à elle seule une validation matérielle du rendu WGPU; la présentation sur GPU réel doit être qualifiée séparément.

## Documentation

- `docs/ARCHITECTURE.md`
- `docs/TRANSFORMS.md`
- `docs/ANIMATION.md`
- `docs/EDITOR.md`
- `docs/PERFORMANCE.md`
- `docs/SECURITY.md`
