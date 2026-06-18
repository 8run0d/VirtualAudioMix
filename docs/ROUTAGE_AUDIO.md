# Guide des blocs et routages

VirtualAudioMix fonctionne avec des blocs reliés par des liens.

Un bloc représente une source ou une destination audio. Un lien indique où le son doit aller.

## Types de blocs

### Entrées physiques

Exemples:

- micro USB;
- entrée d'une carte son;
- webcam avec micro.

Ces blocs peuvent envoyer du son vers une sortie physique, `VAM Entrée` ou `VAM Sortie`.

### Sorties physiques

Exemples:

- casque;
- enceintes;
- carte son;
- interface audio USB.

Ces blocs reçoivent du son.

### `VAM Entrée`

`VAM Entrée` est le microphone virtuel.

Tout ce qui est routé vers `VAM Entrée` peut être utilisé comme micro par Discord, OBS, un navigateur ou un enregistreur.

Exemple:

```text
Micro + VLC -> VAM Entrée
```

### `VAM Sortie`

`VAM Sortie` est la sortie virtuelle Windows.

Si Windows utilise `VAM Sortie` comme sortie par défaut, les sons du PC arrivent dans VirtualAudioMix.

Exemple:

```text
VAM Sortie -> Casque
```

### Processus

Le menu `Processus` affiche les applications qui émettent du son.

Exemples:

```text
VLC -> Casque
Firefox -> VAM Entrée
Jeu -> VAM Sortie
```

Les processus sont traités comme des sources stéréo.

## Créer un lien

1. Ajoutez deux blocs.
2. Tirez un lien depuis le point d'attache de la source.
3. Déposez le lien sur le point d'attache de la destination.
4. Cliquez sur `Démarrer audio`.

## Modifier le volume d'un lien

Chaque lien possède un contrôle de gain.

- `1.00`: volume normal.
- `0.50`: moitié du volume.
- `0.00`: silence.
- au-dessus de `1.00`: amplification.

Évitez les gains trop élevés si le son sature.

## Activer ou désactiver un bloc

Le switch d'un bloc coupe ou réactive ses routes audio sans supprimer les liens.

C'est utile pour tester plusieurs configurations sans reconstruire le graphe.

## Canaux stéréo

Certains blocs peuvent être déployés pour afficher leurs canaux:

- `ALL`: tous les canaux;
- `L`: gauche;
- `R`: droite;
- `Canal 1`, `Canal 2`, etc. pour certaines entrées.

Exemples:

```text
Micro -> OUT L
VAM Sortie -> OUT R
VLC -> VAM Entrée L
Firefox -> VAM Entrée R
```

## Éviter les boucles audio

Une boucle audio arrive quand une sortie est renvoyée vers elle-même.

Exemple risqué:

```text
Sortie physique capturée -> même sortie physique
```

Configuration recommandée pour le son Windows:

```text
Windows: VAM Sortie comme sortie par défaut
VirtualAudioMix: VAM Sortie -> sortie physique
```
