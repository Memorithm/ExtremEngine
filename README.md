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
- `extrem_assets` : handles typés, déduplication par chemin et registre d’assets ;
- `extrem_audio` : commandes audio et contrat de backend ;
- `extrem_editor` : commandes d’inspection, sélection et undo/redo ;
- `extrem_input` : clavier, souris et transitions de boutons sans dépendance plateforme ;
- `extrem_physics` : rigid bodies et résolution sol/gravity en fixed timestep ;
- `extrem_scene` : composants de scène, hiérarchie parent/enfant et propagation des transforms ;
- `extrem_render` : contrat de rendu remplaçable et backend nul pour les tests ;
- `extrem_gpu` : initialisation wgpu headless isolée et exemple de détection GPU ;
- `extrem_window` : hôte de fenêtre natif et boucle d’événements multiplateforme ;
- `extrem_science` : primitives de simulation numérique sans dépendance obligatoire ;
- `extrem_engine` : façade haut niveau et exemple de boucle de jeu.

L’exemple sandbox valide la boucle moteur, les mises à jour ECS, le fixed timestep et l’extraction de commandes de rendu. `extrem_window` fournit l’hôte natif nécessaire au branchement d’un renderer interactif.

## Démarrer

Depuis ce dossier :

```text
cargo test --workspace --all-targets
cargo run -p extrem_engine --example sandbox
cargo run -p extrem_gpu --example probe
```

## Direction technique

Le code est écrit à partir de contrats propres à ExtremEngine. L’archive Bevy fournie sert de référence d’architecture et de conception ; aucun fichier Bevy n’est copié dans ce workspace. Le dépôt public `Memorithm/ExtremEngine` héberge cette première version du noyau.

Le module scientifique n’active pas encore une dépendance `scirust` par défaut. Il expose une interface minimale afin de pouvoir brancher une bibliothèque de calcul spécialisée lorsqu’un besoin concret — intégration ODE, algèbre linéaire, champs, optimisation ou ML — sera défini.

## État de livraison

1. Terminé : fenêtre native, boucle d’événements et détection GPU `wgpu` headless.
2. Terminé : assets typés, scènes RON et propagation de hiérarchie.
3. Terminé : caméras, projections, render graph et renderer CPU de validation.
4. Terminé : physique déterministe, audio abstrait et input clavier/souris.
5. Terminé : inspection ECS, sélection et undo/redo côté éditeur.
6. Terminé : Euler, RK4 et horloge de simulation pour les intégrations scientifiques.
7. Suite : hot reload/importeurs, matériaux/lumières et renderer `wgpu` présentable.
