---
title: Configuration
description: "Every setting Statup reads at start, its default and the values it takes."
---

Every setting is optional. Write the ones you change in a `.env` file next to `docker-compose.yml`, then run `docker compose up -d` again so the container starts with them.

```env
# .env, next to docker-compose.yml
PUBLIC_URL=https://status.example.com
TRUST_PROXY_HEADERS=true
TZ=Europe/Paris
```

> [!NOTE]
> With Docker Compose, `DATABASE_URL`, `UPLOAD_DIR`, `HOST` and `PORT` belong to the image and `docker-compose.yml` sets them, so the data stays in its volume. A value written for them in `.env` is not used.

A value Statup cannot read, such as a port of `0` or a word where a number is expected, stops it at start with the name of the setting in its output (`docker compose logs statup`).

## Address and proxy

### `PUBLIC_URL`

- Default: unset, the address of each request is used
- Values: an address, such as `https://status.example.com`

The address visitors use. Links in the feed and in the alerts use it. An `https://` address marks the session cookie `Secure` and sends HSTS.

### `TRUST_PROXY_HEADERS`

- Default: `false`
- Values: `true`, `false`

Read the client address from `CLIENT_IP_HEADER` and the scheme from `X-Forwarded-Proto`. Only behind a reverse proxy that sets them: see [Behind a reverse proxy](reverse-proxy.md).

### `CLIENT_IP_HEADER`

- Default: `X-Forwarded-For`
- Values: a header name, such as `X-Real-IP`, `Forwarded` or `CF-Connecting-IP`

The header your proxy writes the client address in. Only its last entry counts, and no other header is read. Used only when `TRUST_PROXY_HEADERS` is `true`.

## Access

### `PUBLIC_MODE`

- Default: `false`
- Values: `true`, `false`

Who can see the page until an administrator chooses: `true` for everyone, `false` for members only. Once the choice is made, at the first launch or in Settings, it takes precedence and is kept across restarts.

### `ADMIN_EMAIL`

- Default: unset
- Values: an email address

With `ADMIN_PASSWORD`, creates an administrator at start when no account exists yet. Both are needed. Remove them once the account exists.

### `ADMIN_PASSWORD`

- Default: unset
- Values: a password of 12 characters with upper and lower case letters, a digit and a symbol, or of 20 characters of any kind

The password of the administrator created by `ADMIN_EMAIL`.

### `SESSION_EXPIRY`

- Default: `3600`
- Values: a number of seconds, greater than 0

How long a sign-in form stays valid. Once signed in, a session lasts 30 days without a visit with **Stay signed in for 30 days** ticked, 24 hours otherwise.

## Time and language

### `TZ`

- Default: `UTC`
- Values: a time zone, such as `Europe/Paris`

The time zone used until one is chosen. The zone set in Settings, or taken from the browser that creates the first account, takes precedence. Dates are shown in it, with the UTC offset where it matters, and maintenance times are typed in it.

### `DEFAULT_LOCALE`

- Default: `fr`
- Values: `fr`, `en`, `de`, `es`

The language of visitors whose browser asks for none of these four. `de` and `es` since 0.3.0.

## Monitoring and updates

### `MONITORING`

- Default: `true`
- Values: `true`, `false`
- Since: 0.2.0

Run the checks of the services that have one, every minute. `false` stops them; the addresses stay saved, and **Test** in a service's form still checks an address when pressed. See [Monitoring](monitoring.md).

### `UPDATE_CHECK`

- Default: `true`
- Values: `true`, `false`

Ask GitHub once a day whether a newer version is published, and tell administrators in Settings. The request carries no information about the instance. `false` for an instance that must not reach the internet.

## Email

The mail server of the email alerts and of the [forgotten password](forgotten-password.md) links. Email channels are offered only when `SMTP_HOST` is set.

### `SMTP_HOST`

- Default: unset
- Values: a host name, such as `smtp.example.com`
- Since: 0.3.0

The mail server. Setting it makes `SMTP_FROM` required.

### `SMTP_SECURITY`

- Default: `starttls`
- Values: `starttls`, `tls`, `none`
- Since: 0.3.0

How the connection to the mail server is secured. `none` for a relay on your own network.

### `SMTP_PORT`

- Default: `587` with `starttls`, `465` with `tls`, `25` with `none`
- Values: a port number
- Since: 0.3.0

The port of the mail server, when it is not the usual one for its security.

### `SMTP_USERNAME`

- Default: unset
- Values: a user name
- Since: 0.3.0

With `SMTP_PASSWORD`, when the mail server asks for them. Set both or neither.

### `SMTP_PASSWORD`

- Default: unset
- Values: a password
- Since: 0.3.0

The password that goes with `SMTP_USERNAME`.

### `SMTP_FROM`

- Default: none, required with `SMTP_HOST`
- Values: a sender, such as `Statup <status@example.com>`
- Since: 0.3.0

The sender of every email Statup sends.

## Storage and server

With Docker Compose, the first four belong to the image: `docker-compose.yml` sets them.

### `DATABASE_URL`

- Default: `./statup.db`, and `/data/statup.db` in the image
- Values: a path to an SQLite file

The database. Statup creates it on first start.

### `UPLOAD_DIR`

- Default: `data/uploads`, and `/data/uploads` in the image
- Values: a directory

Where the uploaded icons and the logo are kept.

### `HOST`

- Default: `0.0.0.0`
- Values: an IP address

The address Statup listens on.

### `PORT`

- Default: `3000`
- Values: a port number, greater than 0

The port Statup listens on. With Docker Compose, change the published port under `ports:` instead.

### `DB_MAX_CONNECTIONS`

- Default: `10`
- Values: a number, greater than 0

The size of the database connection pool.

## Logs

### `LOG_LEVEL`

- Default: `info`
- Values: `trace`, `debug`, `info`, `warn`, `error`, `off`

What the server output shows. The [Journal](journal.md) keeps its lines whatever the level.

### `RUST_LOG`

- Default: unset
- Values: a filter, such as `statup=debug,tower_http=info`

A finer filter of the server output. When it holds a valid filter, it replaces `LOG_LEVEL`.
