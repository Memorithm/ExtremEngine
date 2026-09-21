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
- `extrem_gpu` : contexte et surface `wgpu`, rendu natif de maillages indexés opaques avec profondeur, normales, éclairage directionnel Lambert, géométrie partagée et lecture hors écran. Le presenter de triangle reste un outil de validation distinct.
- `extrem_render` : contrat de backend, renderer nul/CPU, render graph itératif avec plans immuables partagés, réemploi des noms de passes et suppression de dépendances pour réparation.
- `extrem_web` : détection et validation des capacités d'exécution Web/WebGPU en contexte sécurisé.
- `extrem_animation` : squelette, clips validés, sampling, nlerp/slerp, blending de poses, palette LBS et contrats transactionnels EEFP/VPAE expérimentaux.
- `extrem_physics` : **solveur de référence minimal** (gravité + sol + box), avec validation des données. Ce n'est pas encore un solveur rigid-body général.
- `extrem_science` : Euler/RK4 avec validation numérique et workspace RK4 réutilisable.
- `extrem_audio` : contrat de commandes/backend audio et backend nul; sortie audio de production encore à implémenter.
- `extrem_engine` : façade haut niveau, propagation et extraction réutilisables, sélection de caméra avant calcul, préparation partagée du render graph et frame fallible avant tout effet de simulation/rendu en cas de graphe invalide. Le backend `WgpuMeshRenderer` rend les composants `MeshInstance` et sélectionne un `DirectionalLight` actif ; le backend historique `WgpuRenderer` reste limité au triangle de validation.

## Programme de performance

`EE-PERF-01` porte sur le validateur hiérarchique. `EE-PERF-02` porte sur la propagation des transformations : suppression des copies des listes d'enfants et réemploi des buffers dans le moteur. `EE-PERF-03` porte sur l'extraction de rendu : suppression de l'ensemble temporaire d'entités, buffer compact réutilisable, tri en place et sélection de caméra avant calcul de matrice. Ces trois séries conservent leur oracle historique et leur microbenchmark CPU avant/après avec contrôle des sorties.

`EE-PERF-04` élimine les copies du plan compilé et des noms de passes sur les frames où le graphe reste inchangé. L'invalidation repose sur l'identité d'un plan partagé vivant, pas sur la seule version du graphe. Le banc compile le même scénario sur l'ancienne et la nouvelle version réelles du moteur et mesure des ticks CPU complets avec `NullRenderer`, y compris un scénario de remplacement du graphe à chaque tick. Méthode, nouvelles API et limites : `docs/RENDER_PLAN_CACHE.md`.

Les workflows `Scene Performance`, `Render Extraction Performance` et `Render Graph Performance` conservent les échantillons bruts, les SHA exécutés et l'environnement dans leurs artefacts. `docs/PERFORMANCE.md` décrit EE-PERF-01/02 ; `docs/RENDER_EXTRACTION.md` décrit EE-PERF-03. La libération explicite du buffer d'extraction est disponible via `Engine::release_render_scratch()`.

Ces mesures CPU sur runners partagés ne constituent ni une qualification GPU ni une promesse de FPS. Les gains, les régressions et leurs limites doivent être évalués à partir des sorties effectivement produites. Le pipeline de maillages est désormais un chemin GPU réel avec normales et un éclairage directionnel simple ; PBR, ombres, transparence, skinning, scènes glTF complètes et qualification matérielle restent des travaux distincts. Textures/UV, instancing consécutif, culling frustum AABB conservateur et import GLB statique (sous-ensemble TRIANGLES) sont déjà présents.

## Rendu natif de maillages

`MeshData::new` valide les sommets/indices et dérive des normales lissées pondérées par l'aire des triangles. `MeshData::new_with_normals` accepte des normales explicites pour les arêtes dures. `MeshInstance` associe la géométrie immuable à une entité et une teinte opaque. `DirectionalLight` fournit une direction monde, une couleur, une intensité et une composante ambiante ; le plus petit identifiant d'entité actif est sélectionné de façon déterministe. Sans lumière, le fallback ambiant reproduit le rendu non éclairé antérieur.

`Engine::with_mesh_renderer` utilise les transformations du monde, la caméra sélectionnée et `Visibility`. Le même pipeline indexé avec profondeur sert une fenêtre ou une cible hors écran. Les normales sont transformées par la co-matrice du modèle afin de supporter les échelles non uniformes et les modèles miroirs ; les bases singulières sont rejetées avant soumission.

```bash
cargo run -p extrem_engine --example mesh_scene --locked
cargo run -p extrem_engine --example mesh_scene --locked -- --headless mesh-cubes.ppm
```

La qualification `Mesh Qualification` exécute des contrôles de pixels avec WGPU, compare l'éclairage à une référence CPU et échoue si aucun adaptateur n'est disponible. Un résultat sur Vulkan logiciel n'est pas une mesure de performance GPU matérielle. Consulter `docs/MESH_RENDERING.md` pour les plafonds, erreurs et limites. PBR, transparence, ombres, skinning et import glTF+URI complets ne sont pas encore implémentés. Textures/UV, instancing consécutif, culling frustum AABB et import GLB statique (sous-ensemble) le sont.

## Erreurs de frame et migration d'API

`Engine::tick(delta)` retourne `Result<UpdateReport, RenderGraphError>` et `Engine::run_for(n)` retourne `Result<Vec<UpdateReport>, RenderGraphError>`. Les appels doivent traiter le résultat, par exemple avec `?`. Un graphe invalide est rejeté avant la simulation et avant tout appel au backend : les entrées et les observations de la dernière frame réussie sont conservées. `RenderGraph::remove_dependency` permet de réparer une dépendance puis de réessayer. La compilation ne dépend plus d'un parcours récursif de profondeur non bornée.

Cette protection concerne les erreurs retournées par le graphe, pas les panics des systèmes utilisateurs, les erreurs d'allocation ou toutes les erreurs GPU. Le contrat complet et les exemples de migration sont dans `docs/RENDER_GRAPH.md`. `compile()` reste compatible et retourne une copie indépendante ; `compile_shared()` fournit le nouveau plan partagé.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test -p extrem_engine -p extrem_render --doc --locked
cargo run -p extrem_engine --example sandbox --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

La CI vérifie également le MSRV Rust 1.87 et compile le workspace sur Linux, Windows et macOS. Les jobs Windows/macOS exécutent aussi les tests de l'éditeur, du moteur et du render graph. Linux exécute les exemples de documentation et le sandbox sans fenêtre. Une compilation réussie ne constitue pas à elle seule une validation matérielle du rendu WGPU; la présentation et les performances sur GPU réel doivent être qualifiées séparément.

## Documentation

- `docs/ARCHITECTURE.md`
- `docs/TRANSFORMS.md`
- `docs/ANIMATION.md`
- `docs/EDITOR.md`
- `docs/PERFORMANCE.md`
- `docs/RENDER_EXTRACTION.md`
- `docs/RENDER_GRAPH.md`
- `docs/RENDER_PLAN_CACHE.md`
- `docs/MESH_RENDERING.md`
- `docs/SECURITY.md`
