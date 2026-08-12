# ExtremEngine

ExtremEngine est un moteur de jeu Rust modulaire en construction. L’objectif est de réunir :

- la simplicité et la modularité d’un moteur data-oriented ;
- des outils de production plus riches à terme, inspirés des moteurs généralistes ;
- une base native Rust, sûre, testable et extensible ;
- un point d’intégration pour les calculs scientifiques et les simulations.

## Premier incrément

Ce dépôt contient actuellement un noyau exécutable :

- `extrem_ecs` : entités, composants, ressources et monde ;
- `extrem_math` : types mathématiques de base et transforms ;
- `extrem_app` : temps, plugins, startup/fixed-update/update/post-update/render schedules ;
- `extrem_scene` : composants de scène, hiérarchie parent/enfant et propagation des transforms ;
- `extrem_render` : contrat de rendu remplaçable et backend nul pour les tests ;
- `extrem_science` : primitives de simulation numérique sans dépendance obligatoire ;
- `extrem_engine` : façade haut niveau et exemple de boucle de jeu.

L’exemple actuel est volontairement sans fenêtre : il valide la boucle moteur, les mises à jour ECS, le fixed timestep et l’extraction de commandes de rendu. Le prochain incrément pourra connecter un backend `wgpu` sans modifier le cœur ECS.

## Démarrer

Depuis ce dossier :

```text
cargo test --workspace --all-targets
cargo run -p extrem_engine --example sandbox
```

## Direction technique

Le code est écrit à partir de contrats propres à ExtremEngine. L’archive Bevy fournie sert de référence d’architecture et de conception ; aucun fichier Bevy n’est copié dans ce workspace. Le dépôt GitHub cible `Memorithm/ExtremEngine` est encore vide au moment de cette première initialisation.

Le module scientifique n’active pas encore une dépendance `scirust` par défaut. Il expose une interface minimale afin de pouvoir brancher une bibliothèque de calcul spécialisée lorsqu’un besoin concret — intégration ODE, algèbre linéaire, champs, optimisation ou ML — sera défini.

## Feuille de route proposée

1. Fenêtre et rendu 2D/3D `wgpu`.
2. Assets, sérialisation de scènes et hot reload.
3. Caméras, lumières, matériaux et render graph.
4. Physique, audio, input et navigation.
5. Éditeur de scènes et inspection ECS.
6. Intégration scientifique ciblée pour les simulations.
