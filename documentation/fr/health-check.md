---
title: Contrôle de santé
description: "L'adresse qu'un répartiteur de charge, un outil de supervision ou Docker peut appeler pour savoir si Statup répond."
---

## `GET /health`

| Réponse | Statut | Quand |
|---|---|---|
| `{"status":"ok"}` | 200 | Statup répond et sa base de données aussi |
| `{"status":"degraded"}` | 503 | La base de données ne répond pas |

Cette adresse n'est soumise à aucune limite de requêtes et n'ouvre aucune session : un outil peut l'appeler aussi souvent qu'il le faut.

## Dans l'image

L'image Docker l'appelle toutes les 30 secondes, après une période de démarrage de 5 secondes, avec un délai d'attente de 3 secondes ; trois échecs de suite marquent le conteneur comme défaillant (unhealthy). `docker compose ps` affiche son état.
