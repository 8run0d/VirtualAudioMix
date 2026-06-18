# VirtualAudioMix

![VirtualAudioMix logo](images/VAM%20Logo.png)

VirtualAudioMix est un routeur audio virtuel pour Windows.

Il permet de connecter visuellement vos micros, sorties audio, sons Windows et applications vers des destinations physiques ou virtuelles.

Exemples d'usage:

- envoyer un micro vers `VAM Entrée` pour Discord, OBS ou un enregistreur;
- écouter le son Windows dans vos enceintes via `VAM Sortie`;
- mixer micro + musique + application vers un micro virtuel;
- router VLC à gauche et Firefox à droite;
- créer et rappeler des presets audio.

## Fonctionnalités

- Interface visuelle par blocs et liens.
- Driver virtuel **Bubux Audio Driver** (`BAD`).
- Endpoints Windows:
  - `VAM Entrée`: microphone virtuel;
  - `VAM Sortie`: sortie virtuelle.
- Routage matériel: micros, entrées carte son, sorties casque/enceintes.
- Routage du son système Windows.
- Routage de processus audio isolés.
- Routage stéréo `L/R` quand le périphérique le permet.
- Gain réglable par lien.
- Presets utilisateur.
- Réduction dans la zone de notification Windows.
- Démarrage automatique avec Windows.

## Installation

Téléchargez l'installateur depuis la page des releases du projet.

Package recommandé:

```text
VirtualAudioMix_0.1.0_x64.msi
```

L'installateur installe:

- VirtualAudioMix;
- le driver audio virtuel BAD;
- les endpoints `VAM Entrée` et `VAM Sortie`.

Windows peut demander une autorisation administrateur. Un redémarrage peut être nécessaire après installation ou mise à jour du driver.

## Premier lancement

Au premier lancement, choisissez:

- votre micro par défaut;
- votre sortie casque/enceintes par défaut.

VirtualAudioMix crée ensuite un graphe de base:

```text
Micro par défaut -> VAM Entrée
VAM Sortie -> sortie par défaut
```

Vous pouvez modifier ce graphe librement.

## Documentation utilisateur

- [Démarrage rapide](docs/DEMARRAGE_RAPIDE.md)
- [Guide des blocs et routages](docs/ROUTAGE_AUDIO.md)
- [Presets et paramètres](docs/PRESETS_ET_PARAMETRES.md)
- [Dépannage](docs/DEPANNAGE.md)

## Confidentialité

VirtualAudioMix fonctionne hors ligne.

Il n'y a pas:

- d'inscription;
- de compte utilisateur;
- de tracker;
- d'appel réseau nécessaire au fonctionnement audio.

## À propos

Projet développé par Bruno Del piero.

Le logiciel est gratuit, open source, développé en Rust et React. Si vous voulez contribuer, modifier ou forker le projet, faites-vous plaisir.

GitHub: <https://github.com/8run0d/VirtualAudioMix>

## Licence

MIT. Voir `LICENSE`.
