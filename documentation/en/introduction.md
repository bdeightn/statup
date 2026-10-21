---
title: Introduction
description: "What Statup is, and the three things it is made of: services, events and people."
---

Statup is a status page you host yourself. Whoever runs a service posts, once, what is broken, what is planned and what has changed. Everyone else reads it in plain words, from a desk or a phone, without an account when the page is open to everyone.

Monitoring tools are made for the people who fix things. Statup is made for the people who use them: it answers "Is it down?" before anyone asks.

## Services

The tools people rely on: mail, the VPN, the intranet, the phones. Each one shows a state: **Operational**, **Degraded**, **Outage** or **Under maintenance**, and its availability day by day. Statup can also check on its own that a service answers. See [Services](services.md) and [Monitoring](monitoring.md).

## Events

What gets published:

- an **incident** when something breaks, with its updates until it is resolved: see [Incidents](incidents.md);
- a **maintenance** when work is planned, which starts and ends on its own at the times announced: see [Maintenance](maintenance.md);
- an **announcement** for news: see [Announcements](announcements.md).

An incident sets the state of the services it names while it is investigated or fixed, and a maintenance while it is under way. Each event can also be posted to Teams, Slack, email and other tools: see [Alerts](alerts.md).

## People

- **Readers** read the page and the events. When the page is **Public**, visitors read it without an account.
- **Editors** publish events and set the state of services.
- **Administrators** also run the settings, the team, the alerts and the layout of the page.

See [Team and roles](team.md).

## What it runs on

One binary and one SQLite file, in a Docker image of about 11 MB, for x86-64 and ARM servers. No other database, no cache to run.

Fonts and scripts come from your instance. The server only reaches the addresses you ask it to check or to alert, and GitHub once a day to look for a new version, which [`UPDATE_CHECK=false`](configuration.md#update_check) turns off.

Statup is free and open source, under the AGPL-3.0. Start with [Install](install.md).
