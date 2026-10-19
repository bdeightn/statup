---
title: Behind a reverse proxy
description: "Serve Statup over HTTPS under a name of your own, with nginx, Caddy or any proxy that sets the usual headers."
---

Statup speaks plain HTTP on port 3000. For HTTPS and a name such as `status.example.com`, put a reverse proxy in front: it holds the certificate and passes the requests on.

## Publish Statup on loopback only

So that nobody reaches Statup around the proxy, publish it on the loopback address of the host. In `docker-compose.yml`, under `ports:`:

```yaml
ports:
  - "127.0.0.1:3000:3000"
```

Then, in `.env`:

```env
PUBLIC_URL=https://status.example.com
TRUST_PROXY_HEADERS=true
```

Run `docker compose up -d` again.

## What the two settings change

- [`PUBLIC_URL`](configuration.md#public_url) is the address in every link Statup writes, in the feed and in the alerts. Its `https://` marks the session cookie `Secure` and sends HSTS.
- [`TRUST_PROXY_HEADERS`](configuration.md#trust_proxy_headers) makes Statup read the client address from the last entry of `X-Forwarded-For`, the one your proxy appended whatever a client sent before it, and the scheme from `X-Forwarded-Proto`. The address is what the rate limits and the [Journal](journal.md) use.

nginx, Caddy, Traefik, Apache and HAProxy (with `option forwardfor`) all append the address to `X-Forwarded-For`. If your proxy writes it in another header, name that header in [`CLIENT_IP_HEADER`](configuration.md#client_ip_header).

> [!WARNING]
> Never set `TRUST_PROXY_HEADERS=true` when Statup can be reached directly: anyone could then choose their own address and walk around the rate limits.

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

Caddy sets both headers on its own:

```caddy
status.example.com {
    reverse_proxy 127.0.0.1:3000
}
```
