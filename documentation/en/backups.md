---
title: Backups
description: "What to save, and how, so that an instance can be rebuilt on another server."
---

Everything Statup keeps lives in two places: the SQLite database, with its `-wal` and `-shm` files, and the uploads directory, which holds the logo and the icons. With Docker, both are in the `statup_data` volume.

Compose names the volume after the project folder: `statup_statup_data` for a folder called `statup`. `docker volume ls` lists the exact name.

## With Docker

Stop Statup, archive the volume, start it again:

```bash
docker compose stop statup
docker run --rm -v statup_statup_data:/data -v "$PWD":/backup alpine \
  tar czf /backup/statup-backup.tar.gz -C /data .
docker compose start statup
```

`statup-backup.tar.gz` lands in the current folder.

## From source

Stop the server, then copy `statup.db`, `statup.db-wal`, `statup.db-shm` and `data/uploads/`.

## What a backup contains

The accounts, the events, the services, the settings, the [Journal](journal.md), and the addresses of the alert channels. Anyone who knows a webhook address can post in its channel: keep the backups where you keep other secrets.

> [!IMPORTANT]
> Back up before every [upgrade](upgrading.md). A database migrated by a newer version is refused by an older one.
