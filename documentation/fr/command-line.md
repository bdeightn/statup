---
title: Ligne de commande
description: "Les commandes que le binaire Statup exécute sur le serveur, à côté de l'application web."
---

Lancé sans argument, le binaire démarre le serveur. Il n'existe qu'une seule commande de maintenance.

## `reset-password`

Donne un mot de passe provisoire à un compte, et l'affiche.

```bash
docker compose exec statup /app/statup reset-password you@example.com
```

Depuis les sources, à côté de votre `.env` :

```bash
./target/release/statup reset-password you@example.com
```

- La commande lit `.env` et `DATABASE_URL` comme le serveur, et ne travaille que sur une base existante : un `DATABASE_URL` mal saisi est signalé comme tel, et aucune base vide n'est créée au passage.
- Elle met d'abord la base au niveau de la version du binaire, comme le fait le serveur au démarrage.
- Seuls les comptes actifs peuvent être réinitialisés. Le mot de passe provisoire est valable sept jours ; la personne choisit le sien à sa prochaine connexion, et ses sessions ouvertes sont fermées.

Codes de sortie : `0` quand le mot de passe est donné, `1` en cas d'échec, par exemple un compte introuvable ou une base impossible à ouvrir, `2` quand la commande n'est pas comprise.

## `--help`

`statup --help`, ou `-h`, affiche la liste des commandes.
