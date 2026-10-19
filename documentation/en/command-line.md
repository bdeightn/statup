---
title: Command line
description: "The commands the Statup binary runs on the server, next to the web app."
---

The binary starts the server when it is run without arguments. One maintenance command exists.

## `reset-password`

Gives an account a temporary password, and prints it.

```bash
docker compose exec statup /app/statup reset-password you@example.com
```

From source, next to your `.env`:

```bash
./target/release/statup reset-password you@example.com
```

- It reads `.env` and `DATABASE_URL` like the server, and works on an existing database only: a mistyped `DATABASE_URL` is reported as such, and no empty database is left behind.
- It brings the database to the binary's version first, as the server does at start.
- Only active accounts can be reset. The temporary password is valid for seven days; the person chooses their own at the next sign-in, and their open sessions are signed out.

Exit codes: `0` when the password is given, `1` when it fails, such as an account not found or a database that cannot be opened, `2` when the command is not understood.

## `--help`

`statup --help`, or `-h`, prints the list of commands.
