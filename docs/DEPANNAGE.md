# Dépannage

## Je ne vois pas `VAM Entrée` ou `VAM Sortie`

Causes possibles:

- le driver BAD n'est pas installé;
- Windows n'a pas encore chargé le driver;
- un redémarrage est nécessaire.

Actions:

1. Redémarrez Windows.
2. Vérifiez dans les paramètres son Windows.
3. Relancez VirtualAudioMix.
4. Réinstallez l'application si les endpoints n'apparaissent toujours pas.

## Je n'entends pas le son Windows

Vérifiez que Windows utilise `VAM Sortie` comme sortie par défaut.

Configuration attendue:

```text
Windows -> VAM Sortie
VirtualAudioMix: VAM Sortie -> casque/enceintes
```

Si Windows sort directement vers vos enceintes, VirtualAudioMix ne reçoit pas le son système via `VAM Sortie`.

## Discord, OBS ou Magnétophone ne reçoit rien

Vérifiez que le logiciel cible utilise `VAM Entrée` comme microphone.

Dans VirtualAudioMix, il faut aussi router une source vers `VAM Entrée`.

Exemple:

```text
Micro -> VAM Entrée
```

## J'entends le son des deux côtés alors que j'ai choisi `L` ou `R`

Vérifiez que le lien arrive bien sur le sous-canal voulu:

- `L`: gauche;
- `R`: droite;
- `ALL`: les deux canaux.

Déployez le bloc de destination pour contrôler le point d'attache utilisé.

## Une application n'apparaît pas dans `Processus`

Le menu `Processus` affiche les applications qui ont une session audio active.

Actions:

1. Lancez un son dans l'application.
2. Attendez quelques secondes.
3. Rafraîchissez la liste si nécessaire.

Certains navigateurs regroupent plusieurs onglets dans une même session audio Windows.

## Le volume Windows ne change rien

La configuration recommandée est:

```text
Windows: sortie par défaut = VAM Sortie
VirtualAudioMix: VAM Sortie -> sortie physique
```

Dans cette configuration, le volume Windows agit sur le flux `VAM Sortie`.

## L'installateur affiche `Éditeur inconnu`

Windows affiche `Éditeur inconnu` tant que l'application et l'installateur ne sont pas signés avec un certificat Authenticode valide.

Ce message ne signifie pas forcément que l'application est dangereuse, mais il indique que Windows ne peut pas vérifier officiellement l'éditeur.

## Windows demande une autorisation administrateur

C'est normal: VirtualAudioMix installe un driver audio virtuel.

Le driver est nécessaire pour créer:

- `VAM Entrée`;
- `VAM Sortie`.

## Désinstaller VirtualAudioMix

Utilisez les paramètres Windows:

```text
Applications installées -> VirtualAudioMix -> Désinstaller
```

La désinstallation tente aussi de supprimer le driver BAD.

Un redémarrage peut être nécessaire si Windows garde encore le driver chargé.

## Après désinstallation, les périphériques VAM sont encore visibles

Redémarrez Windows.

Si les périphériques restent visibles après redémarrage, réinstallez puis désinstallez VirtualAudioMix, ou ouvrez une issue sur GitHub avec les détails de votre configuration.
