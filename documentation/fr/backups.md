---
title: Sauvegardes
description: "Ce qu'il faut sauvegarder, et comment, pour pouvoir reconstruire une instance sur un autre serveur."
---

Tout ce que Statup conserve tient en deux endroits : la base SQLite, avec ses fichiers `-wal` et `-shm`, et le répertoire des fichiers envoyés, qui contient le logo et les icônes. Avec Docker, les deux sont dans le volume `statup_data`.

Compose nomme le volume d'après le dossier du projet : `statup_statup_data` pour un dossier nommé `statup`. `docker volume ls` donne le nom exact.

## Avec Docker

Arrêtez Statup, archivez le volume, puis relancez-le :

```bash
docker compose stop statup
docker run --rm -v statup_statup_data:/data -v "$PWD":/backup alpine \
  tar czf /backup/statup-backup.tar.gz -C /data .
docker compose start statup
```

`statup-backup.tar.gz` est créé dans le dossier courant.

## Depuis les sources

Arrêtez le serveur, puis copiez `statup.db`, `statup.db-wal`, `statup.db-shm` et `data/uploads/`.

## Ce que contient une sauvegarde

Les comptes, les événements, les services, les paramètres, le [Journal](journal.md), et les adresses des canaux d'alerte. Quiconque connaît l'adresse d'un webhook peut publier dans son canal : rangez les sauvegardes là où vous gardez vos autres secrets.

> [!IMPORTANT]
> Sauvegardez avant chaque [montée de version](upgrading.md). Une base migrée par une version plus récente est refusée par une version plus ancienne.
