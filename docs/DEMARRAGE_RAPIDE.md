# Démarrage rapide

Ce guide explique comment installer VirtualAudioMix et obtenir un premier routage fonctionnel.

## 1. Installer l'application

Lancez:

```text
VirtualAudioMix_0.1.0_x64.msi
```

Windows peut demander une autorisation administrateur, car l'application installe un driver audio virtuel.

Après installation, vérifiez dans les paramètres son Windows que deux périphériques existent:

- `VAM Entrée`
- `VAM Sortie`

Si Windows le demande, redémarrez le PC.

## 2. Premier lancement

Au premier lancement, VirtualAudioMix demande:

- votre micro par défaut;
- votre sortie casque/enceintes par défaut.

Ces choix servent à créer un graphe de base.

## 3. Configurer Windows

Pour récupérer le son du PC dans VirtualAudioMix:

1. Ouvrez les paramètres son Windows.
2. Choisissez `VAM Sortie` comme sortie Windows par défaut.
3. Dans VirtualAudioMix, routez `VAM Sortie` vers vos enceintes ou votre casque.

Pour envoyer du son vers un logiciel comme Discord, OBS ou Magnétophone:

1. Choisissez `VAM Entrée` comme micro dans le logiciel cible.
2. Dans VirtualAudioMix, routez une source vers `VAM Entrée`.

## 4. Exemple simple: micro vers Discord

Dans VirtualAudioMix:

```text
Micro -> VAM Entrée
```

Dans Discord:

```text
Microphone: VAM Entrée
```

Cliquez ensuite sur `Démarrer audio`.

## 5. Exemple simple: son Windows vers casque

Dans Windows:

```text
Sortie par défaut: VAM Sortie
```

Dans VirtualAudioMix:

```text
VAM Sortie -> votre casque ou vos enceintes
```

Cliquez ensuite sur `Démarrer audio`.

## 6. Arrêter l'audio

Cliquez sur le bouton d'arrêt dans la barre supérieure.

Si l'application est réduite dans la zone de notification, clic droit sur l'icône puis utilisez l'option du moteur audio.
