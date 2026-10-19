---
title: Mot de passe oublié
description: "Comment un membre retrouve l'accès, par courriel ou par un administrateur, et comment réinitialiser n'importe quel compte depuis le serveur."
---

## Par courriel

Avec un serveur de courriel réglé ([`SMTP_HOST`](configuration.md#smtp_host) et les autres) et l'adresse de la page connue ([`PUBLIC_URL`](configuration.md#public_url), ou **Lien dans les messages** en bas de la page [Alertes](alerts.md)), les membres réinitialisent eux-mêmes leur mot de passe : **Mot de passe oublié ?** sur la page de connexion leur envoie un lien par courriel.

- Le lien ne sert qu'une fois, pendant une heure.
- Choisir un nouveau mot de passe connecte le membre et ferme toutes les autres sessions du compte.
- La page répond de la même façon que l'adresse ait un compte ou non, et un compte reçoit au plus trois liens par heure.

## Par un administrateur

Sans serveur de courriel, la même page invite le membre à s'adresser à un administrateur, qui lui donne un mot de passe provisoire depuis la page [Équipe](team.md). Son lien **Réinitialiser depuis le serveur** mène à la partie suivante.

## Depuis le serveur

Si vous hébergez Statup, vous pouvez donner un mot de passe provisoire à n'importe quel compte actif, même quand aucun administrateur ne peut se connecter :

```bash
docker compose exec statup /app/statup reset-password you@example.com
```

Transmettez-le dans les sept jours. La personne s'en sert pour se connecter, puis doit choisir le sien, et toute session encore ouverte sur ce compte est fermée. Voir [Ligne de commande](command-line.md).
