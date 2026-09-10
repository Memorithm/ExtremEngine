# ExtremEngine

ExtremEngine est un moteur de jeu Rust modulaire, sûr et déterministe. Il fournit une architecture propre pour le rendu GPU (`wgpu`), l'animation squelettique, la physique, l'input et les systèmes interactifs.

## Modules du Workspace

- `extrem_ecs` : Entités générationnelles, composants, ressources et monde.
- `extrem_math` : Vecteurs `Vec3`, quaternions `Quat`, matrices `Mat4` et compositior affine `Transform`.
- `extrem_scene` : Hiérarchie parent/enfant avec validation anti-cycle, caméras et documents RON.
- `extrem_editor` : Moteur de transactions `CommandRecord`, historique undo/redo et inspection.
- `extrem_assets` : Normalisation canonique de chemins, détection de collisions et handles typés.
- `extrem_app` : Boucle temporelle déterministe, bornes d'accumulation anti-spirale et schedules.
- `extrem_input` : Gestion d'input clavier, boutons de souris, position/delta et molette.
- `extrem_window` : Intégration de la boucle d'événements `winit` et liaison avec les événements d'entrée.
- `extrem_gpu` : Initialisation `wgpu` (headless + `SurfaceTarget` de rendu de fenêtre).
- `extrem_render` : Render graph avec compilation mise en cache, tri topologique et backends de rendu.
- `extrem_animation` : Squelettes, clips, échantillonnage slerp, blending de poses, palette de matrices pour LBS GPU et interfaces adaptatives.
- `extrem_physics` : Rigid bodies, gravité et collisionneurs box en fixed timestep.
- `extrem_science` : Intégration numérique d'EDO (Euler, RK4) et horloge de simulation.
- `extrem_audio` : Backend audio abstrait pour les tests et la production.
- `extrem_engine` : Façade d'intégration du moteur.

## Commandes de Validation

```bash
cargo test --workspace --all-targets --locked
cargo run -p extrem_engine --example sandbox
cargo run -p extrem_gpu --example probe
```

## Documentation

- `docs/ARCHITECTURE.md`
- `docs/TRANSFORMS.md`
- `docs/ANIMATION.md`
- `docs/SECURITY.md`
