---
title: Health check
description: "The address a load balancer, a monitoring tool or Docker can call to know that Statup answers."
---

## `GET /health`

| Answer | Status | When |
|---|---|---|
| `{"status":"ok"}` | 200 | Statup answers and its database responds |
| `{"status":"degraded"}` | 503 | The database does not respond |

It is not rate limited and opens no session, so it can be called as often as a tool needs.

## In the image

The Docker image calls it every 30 seconds, after a start period of 5 seconds, with a timeout of 3 seconds; three failures in a row mark the container unhealthy. `docker compose ps` shows its state.
