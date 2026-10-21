---
title: Settings
description: "The page's name, logo, time zone and audience, the version, and the layout of the dashboard."
---

**Settings**, for administrators, opens on four tiles, under **Manage**:

- **Team**: the members and their roles. See [Team and roles](team.md).
- **Alerts**: the channels that receive the news. See [Alerts](alerts.md).
- **Journal**: what happened on the instance. See [Journal](journal.md).
- **Version**: the version running.

## General

One form, saved with one **Save** button. Every field is checked before any is written, and leaving the page with changes not saved makes the browser ask first.

| Field | Description |
|---|---|
| **Page name** | Shown instead of Statup in the header and the browser tab. Up to 40 characters; empty shows Statup. |
| **Logo** | Takes the place of the Statup mark in the header; "Powered by Statup" stays at the foot. PNG, JPG, WebP or SVG, up to 256 KB. An image larger than 256 pixels is reduced. **Remove the logo** brings the mark back at once. |
| **Time zone** | Times on the page, on events and on maintenance are shown and typed in it. |
| **Page access** | **Members only** or **Public**. See [Team and roles](team.md#page-access). |

What is saved here takes precedence over [`TZ`](configuration.md#tz) and [`PUBLIC_MODE`](configuration.md#public_mode), and is kept across restarts.

## Version

The **Version** tile shows the version running. When a newer one is published, it says "Version 0.7.0 is available." and links to what changed.

Statup asks GitHub once a day, the first time a minute after it starts. The answer is kept in memory: after a restart, nothing shows until that first question. [`UPDATE_CHECK=false`](configuration.md#update_check) turns it off.

## Arrange the dashboard

**Arrange**, beside **New event** on the dashboard, lets an administrator lay out the page that everyone reads. It is offered on screens 768 pixels wide or more: a phone stacks every block in one column.

| Block | Description |
|---|---|
| **Status banner** | Always shown, full width, above the others. |
| **Services** | A quarter of the width by default. Its options choose the services it lists. A service left out comes back as soon as something is wrong with it. |
| **Recent activity** | Half the width by default. Its options choose what it lists: **Minor incidents**, **Major incidents**, **Maintenance**, **Announcements**. All but maintenance by default. |
| **Maintenance** | A quarter of the width by default. |

- Move a block by dragging it, or with the arrow keys.
- Set its width: **A quarter**, **Half** or **Full width**.
- **Settings**, on a block, opens its options. **Hide** takes it off the page; **Add a block** brings it back.

Every change is saved at once, for every reader: there is one layout per instance. **Done**, or Escape, leaves the mode.
