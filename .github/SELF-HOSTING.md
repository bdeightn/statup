# Self-hosting Statup

Everything past the [quick start](../README.md#quick-start): who can do what, the settings, a reverse proxy in front, backups, upgrades and recovery.

## Reaching Statup from your network

`docker-compose.yml` publishes Statup on port 3000 of the host, so your colleagues reach it at the server's address. To use another port, change the part before `:3000` under `ports:` and run `docker compose up -d` again.

The first account created becomes the administrator: create yours right after the first start, or preset it with `ADMIN_EMAIL` and `ADMIN_PASSWORD`.

The time zone of the instance is taken from the browser that creates the first account, and can be changed later in Settings.

## Access and roles

| Role | Can |
|---|---|
| Reader | Read the dashboard, the events and the feed, edit their own profile |
| Editor | Everything a reader can, plus publish incidents, maintenances and announcements, set service states, manage services, templates and icons |
| Administrator | Everything, plus the settings, the team, the dashboard blocks, and changing or deleting closed events |

Who can see the page is chosen at the first launch, then in **Settings, Who can see the page**:

- **Everyone**: visitors read the page and the feed without an account.
- **Members only**: visitors are asked to sign in, and feed readers can no longer read the feed.

Accounts are created by an administrator on the **Team** page, with a temporary password shown once and valid for seven days; the member chooses their own at first sign-in. Self-registration only exists on an empty instance, for its first administrator.

A chosen password needs 12 characters with upper and lower case letters, a digit and a symbol, or 20 characters of any kind.

## Groups and who sees a service

By default every service is seen by everyone who can see the page. To keep a service for some colleagues, an administrator creates groups in **Settings → Team** and adds members from each group's own page. Each service then says who sees it, in its form:

- **Everyone**, the default.
- **Signed-in members**, offered only when the page is open to everyone: when it is for members only, it would mean the same.
- **Some groups**, ticked in the form. Only the members of those groups see the service.

Editors and administrators always see every service.

- Incidents, maintenance and announcements follow their services: anyone who sees one of the services an event names sees the event. An announcement that names no service is for everyone.
- Someone who may not see a service finds no trace of it on the page, in the events, in the search or in the feed, and a direct link answers "not found". The Atom feed always shows what a visitor without an account sees.
- The text of an event (title, description, updates) is not filtered: everyone who sees the event reads it as written. Name a restricted service in that text only when everyone who sees the event may know about it.
- Deleting a group takes its members out of it. A service that only this group could see is then seen by editors and administrators only, until its form says otherwise; the group's page says how many before you delete it.

## Configuration

Every setting is optional. Write the ones you change in a `.env` file next to `docker-compose.yml`; [`.env.example`](../.env.example) lists them with an explanation each. With Docker Compose, `DATABASE_URL`, `UPLOAD_DIR`, `HOST` and `PORT` belong to the image, so the data stays in its volume.

| Variable | Default | Description |
|---|---|---|
| `TZ` | `UTC` | Time zone used until one is chosen, e.g. `Europe/Paris`. The zone set in Settings, or taken from the first account's browser, takes precedence. Dates are shown in it, with the UTC offset where it matters, and maintenance times are typed in it |
| `PUBLIC_URL` | request host | Address visitors use, e.g. `https://status.example.com`. Feed and notification links use it; an `https://` address marks the session cookie `Secure` and sends HSTS |
| `TRUST_PROXY_HEADERS` | `false` | Read the client address from `CLIENT_IP_HEADER` and the scheme from `X-Forwarded-Proto`. Only behind a reverse proxy that sets them |
| `CLIENT_IP_HEADER` | `X-Forwarded-For` | The header your proxy writes the client address in, such as `X-Real-IP`, `Forwarded` or `CF-Connecting-IP`. Only its last entry counts, and no other header is read |
| `PUBLIC_MODE` | `false` | Starting public access. Once an administrator chooses in Settings, that choice is kept across restarts |
| `UPDATE_CHECK` | `true` | Ask GitHub once a day whether a newer version is published, and tell administrators in Settings. The request carries no information about the instance. `false` for an instance that must not reach the internet |
| `MONITORING` | `true` | Run the monitoring of the services that have it. `false` turns every check off; the addresses stay saved |
| `SMTP_HOST`, `SMTP_PORT`, `SMTP_SECURITY`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM` | unset | The mail server of the email notifications, see [Notifications](#notifications) |
| `DEFAULT_LOCALE` | `fr` | `fr`, `en`, `de` or `es`, for visitors whose browser asks for none of them |
| `ADMIN_EMAIL`, `ADMIN_PASSWORD` | unset | Create an administrator at start when no account exists. Both are needed, and the password follows the rule above. Remove them afterwards |
| `DATABASE_URL` | `./statup.db` | SQLite database file |
| `UPLOAD_DIR` | `data/uploads` | Where uploaded icons and the logo are kept |
| `HOST` | `0.0.0.0` | Listen address, an IP address |
| `PORT` | `3000` | Listen port |
| `SESSION_EXPIRY` | `3600` | Seconds a sign-in form stays valid. Once signed in, a session lasts 30 days without a visit with "Stay signed in", 24 hours otherwise |
| `DB_MAX_CONNECTIONS` | `10` | Database pool size |
| `LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` or `off`. Sets what the server output shows; the Journal keeps its lines whatever the level |
| `RUST_LOG` | unset | Finer log filter, e.g. `statup=debug,tower_http=info`. Replaces `LOG_LEVEL` when set |

## Journal

**Settings → Journal**, for administrators, lists what happened, day by day, and the server output (`docker compose logs`) shows the same lines in English.

- **Essential**, the default, keeps the large changes and the real problems: services added, changed or deleted, settings, alert channels, members added, their role or their account changed, groups created, renamed or deleted and who joins or leaves them, who sees a service, an event deleted, an outage detected and its end, a message to an alert channel given up, sign-ins blocked after too many attempts, and the app's errors.
- **Detailed** keeps everything from the moment it is chosen: sign-ins and refused attempts, password resets, events published and updated, each failed check, each message sent.
- Lines are kept 90 days by default, from 1 to 365, under **Change**. Nobody can edit or delete a line; only the retention removes them. **Export** downloads the lines shown, filters included, as a spreadsheet.
- Sign-in lines keep the client address, read like the rest of the app (see `TRUST_PROXY_HEADERS`). No password, reset link, webhook address or anything after the host of a checked address is ever written.

## Monitoring

A service is monitored from its form, under **Monitoring**: **Web** for an address that should answer (`https://intranet.example.com`), **Port** for a host and its port (`192.168.1.1` and `443`), such as a server, or a router or firewall by its administration port. **Test** runs the check at once and says why it fails: port closed, no answer, unknown name, certificate refused or a server error.

- Statup checks every minute, from the server it runs on, so internal addresses work. A web address that answers with any status below 500 counts as answering, even a refusal such as 401 or 404. Tick **Internal certificate** for a tool whose certificate no public authority signed.
- After three minutes without an answer (one check a minute), the service shows an outage, marked "detected automatically", and comes back as soon as it answers. Statup never writes an incident itself: the team declares one in a click, from the banner or the Services page, with the service and the start already filled in.
- Nothing is declared during a maintenance that takes the service down, nor when every checked service fails at once, which points at Statup's own connection rather than at the services. If that lasts five minutes, the checks count again.
- Services that fall in the same minute, such as the services of a server being restarted, are told in one message per destination ("3 services are not answering", with their names), and their return in one more. Declaring a maintenance before a planned restart keeps the checks quiet altogether.
- Each service sets its own delay under **Monitoring**: "Declare an outage after [3] min without an answer", from 1 to 60 minutes. A critical server can show its outage after a minute, a printer after fifteen. Changing the delay leaves an outage already shown as it is.
- A detected outage of fifteen minutes or more counts in the service's thirty days. Choosing **None** or changing the address of a service in a false outage clears it, from its state and from the thirty days.
- The checks only reach the addresses you set, never read a page's content, and keep no credentials: an address with a user name or a password is refused.

## Notifications

Statup posts each incident, maintenance and announcement where people already read. An administrator adds the destinations in **Settings, Notifications**: the tool, its address, the language of the messages, and what it receives: incidents, maintenance, announcements, and the outages the checks detect. **Send a test** posts a message at once and says why it fails.

| Tool | The address to paste |
|---|---|
| Microsoft Teams | In the channel: ⋯, Workflows, then the template that posts a webhook request to the channel. The former incoming webhooks (Office 365 connectors) stopped in May 2026 |
| Slack | On api.slack.com/apps, create an app, turn on Incoming Webhooks and add it to the channel. Rocket.Chat takes the same format: choose Slack |
| Google Chat | In the space: Apps & integrations, Webhooks, Add |
| Discord | In the channel settings: Integrations, Webhooks, New Webhook, Copy Webhook URL |
| Mattermost | Integrations, Incoming Webhooks, Add |
| ntfy | The topic address, such as `https://ntfy.sh/your-topic`, then subscribe to the topic in the ntfy app. A topic on ntfy.sh is readable by anyone who knows its name: choose one nobody guesses, or use your own ntfy server, with `?auth=` in the address for a protected topic |
| Email | Addresses separated by commas, 50 at most. Each one receives a copy of its own, so an address the server refuses keeps the message from no one else. Needs a mail server, below |
| Any other tool | An address that takes a JSON `POST`: Zapier, Make, n8n, Power Automate, Home Assistant. The body is described below |

- Messages leave within seconds, in order for each destination. One that does not answer is tried again after 30 seconds, 2, 10 and 30 minutes. After that, or at once when the tool refuses the address (404, 401), its waiting messages are given up and the list of destinations says why.
- Links in the messages lead to `PUBLIC_URL`, or else to the address at the foot of the Notifications page: the one the first destination was added from, which you can change.
- Anyone who knows a webhook address can post in its channel. It is kept in the database, shown only to administrators and never written to the logs; your backups contain it.

### Email

Set your mail server in `.env`. Email destinations are offered only when `SMTP_HOST` is set. The same server sends the links of [forgotten passwords](#forgotten-password), and reminds a member when an update they announced on an incident is more than 5 minutes late (once, with a link to the incident when the address of the page is known).

| Variable | Default | Description |
|---|---|---|
| `SMTP_HOST` | unset | The mail server, e.g. `smtp.example.com` |
| `SMTP_SECURITY` | `starttls` | `starttls`, `tls`, or `none` for a relay on your own network |
| `SMTP_PORT` | by security | 587 for `starttls`, 465 for `tls`, 25 for `none` |
| `SMTP_USERNAME`, `SMTP_PASSWORD` | unset | Both or neither, when the server asks for them |
| `SMTP_FROM` | required | The sender, e.g. `Statup <status@example.com>` |

Every message of an event has the same subject and joins one conversation in the mail client.

### The body sent to any other tool

A `POST` with a JSON body, version 1:

```json
{
  "version": 1,
  "happening": "opened",
  "occurred_at": "2026-10-07T09:00:00Z",
  "instance": "Example",
  "event": {
    "id": 7,
    "kind": "incident",
    "severity": "critical",
    "category": null,
    "state": "investigating",
    "title": "Network down at the head office",
    "services": ["Network", "VPN"],
    "planned_start": null,
    "planned_end": null,
    "url": "https://status.example.com/events/7"
  },
  "service": null,
  "message": {
    "mark": "outage",
    "headline": "Major incident · Network down at the head office",
    "summary": "Investigating · Network, VPN",
    "text": "The provider is on site.",
    "url": "https://status.example.com/events/7"
  }
}
```

- `happening`: `opened`, `updated`, `rescheduled`, `started`, `closed`, `published`, `reviewed`, `service_down`, `service_up` or `test`. `reviewed` is the report written once an incident is resolved, sent once; `message.text` holds its start.
- `event`: `kind` is `incident`, `maintenance` or `publication`; `severity` is `minor` or `critical`; `category` is `changelog` or `info`; `state` is the state the event moved to. It is `null` for an outage the checks found, which fills `services` instead, one entry per service of the wave: `[{"name": "Mail", "down_since": "2026-10-07T08:57:00Z"}]`. When the message is about one service alone, `service` holds that same entry; it is `null` for a wave.
- `message`: the text the chat tools receive, in the language of the destination; `mark` is `outage`, `degraded`, `maintenance`, `restored` or `plain`.
- Dates are in UTC. A later version adds fields without changing these.

## Running behind a reverse proxy

Terminate TLS at the proxy, publish Statup on loopback only (`"127.0.0.1:3000:3000"` under `ports:`), so nobody reaches it around the proxy, then set:

```bash
PUBLIC_URL=https://status.example.com
TRUST_PROXY_HEADERS=true
```

Statup then takes the client address from the last entry of `X-Forwarded-For`: the one your proxy appended, whatever a client sent before it. nginx, Caddy, Traefik, Apache and HAProxy (with `option forwardfor`) all append it. If your proxy writes the address in another header, name that header in `CLIENT_IP_HEADER`.

With nginx:

```nginx
location / {
    proxy_pass http://127.0.0.1:3000;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

With Caddy, which sets both headers on its own:

```caddy
status.example.com {
    reverse_proxy 127.0.0.1:3000
}
```

Never enable `TRUST_PROXY_HEADERS` when Statup can be reached directly: anyone could then pick their own address and walk around the rate limits.

## Backups

Everything lives in the SQLite database (with its `-wal` and `-shm` files) and the uploads directory. With Docker, both are in the `statup_data` volume, which Compose names after the project folder: `statup_statup_data` for a clone called `statup`.

```bash
docker compose stop statup
docker run --rm -v statup_statup_data:/data -v "$PWD":/backup alpine \
  tar czf /backup/statup-backup.tar.gz -C /data .
docker compose start statup
```

From source, stop the server and copy `statup.db*` and `data/uploads/`.

## Upgrading

```bash
docker compose pull
docker compose up -d
```

`latest` follows each new version. To upgrade only when you choose, replace `latest` with a version number, such as `0.1.0`, under `image:` in `docker-compose.yml`; the versions are listed in the [changelog](CHANGELOG.md).

Database migrations are compiled into the binary and run at start. Back up first: a database migrated by a newer version is refused by an older one.

## Forgotten password

With a mail server set (`SMTP_HOST` and the others, see [Email](#email)) and the address of the page known (`PUBLIC_URL`, or the one at the foot of the Notifications page), members reset their own password: "Forgot your password?" on the sign-in page emails them a link that works once, for one hour. Choosing a new password signs them in and signs out every other session of the account. The page answers the same whether the address has an account or not, and an account receives at most three links an hour.

Without a mail server, that page asks the member to see an administrator, who gives a temporary password from the Team page. As the host, you can also give one from the server:

```bash
docker compose exec statup /app/statup reset-password you@example.com
# From source, next to your .env: ./target/release/statup reset-password you@example.com
```

Hand it over within seven days. The person signs in with it and is asked to choose their own, and any session still open on that account is signed out. Only active accounts can be reset, and the command never creates a database: a mistyped `DATABASE_URL` is reported as such.

## Health check

`GET /health` answers `{"status":"ok"}` with 200, or `{"status":"degraded"}` with 503 when the database does not respond. It is not rate limited and opens no session.
