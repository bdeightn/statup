---
title: Derrière un reverse proxy
description: "Servir Statup en HTTPS sous un nom à vous, avec nginx, Caddy ou tout proxy qui pose les en-têtes habituels."
---

Statup parle en HTTP simple sur le port 3000. Pour le HTTPS et un nom comme `status.example.com`, placez un reverse proxy devant : il porte le certificat et transmet les requêtes.

## Publier Statup sur la boucle locale uniquement

Pour que personne ne joigne Statup en contournant le proxy, publiez-le sur l'adresse de boucle locale de l'hôte. Dans `docker-compose.yml`, sous `ports:` :

```yaml
ports:
  - "127.0.0.1:3000:3000"
```

Puis, dans `.env` :

```env
PUBLIC_URL=https://status.example.com
TRUST_PROXY_HEADERS=true
```

Relancez `docker compose up -d`.

## Ce que changent ces deux réglages

- [`PUBLIC_URL`](configuration.md#public_url) est l'adresse de chaque lien écrit par Statup, dans le flux et dans les alertes. Son `https://` marque le cookie de session `Secure` et active HSTS.
- [`TRUST_PROXY_HEADERS`](configuration.md#trust_proxy_headers) fait lire à Statup l'adresse du client dans la dernière entrée de `X-Forwarded-For`, celle que votre proxy a ajoutée, quoi qu'un client ait envoyé avant elle, et le schéma dans `X-Forwarded-Proto`. C'est cette adresse qu'utilisent les limites de requêtes et le [Journal](journal.md).

nginx, Caddy, Traefik, Apache et HAProxy (avec `option forwardfor`) ajoutent tous l'adresse à `X-Forwarded-For`. Si votre proxy l'écrit dans un autre en-tête, indiquez cet en-tête dans [`CLIENT_IP_HEADER`](configuration.md#client_ip_header).

> [!WARNING]
> Ne réglez jamais `TRUST_PROXY_HEADERS=true` quand Statup est joignable directement : n'importe qui pourrait alors choisir sa propre adresse et contourner les limites de requêtes.

## nginx

```nginx
location / {
    proxy_pass http://127.0.0.1:3000;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

## Caddy

Caddy pose les deux en-têtes de lui-même :

```caddy
status.example.com {
    reverse_proxy 127.0.0.1:3000
}
```
