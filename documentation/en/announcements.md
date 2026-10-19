---
title: Announcements
description: "Tell people what is new or what they should know, without touching the state of any service."
---

An announcement is news: a new version of a tool, a printer that moved, a client everyone must install. Editors and administrators publish one from **New event**, with **Type** set to **Announcement**.

## Publish an announcement

| Field | Description |
|---|---|
| **Category** | **Information**, by default, or **What's new**. |
| **Services affected** | Optional. They show as tags; their state never changes. |
| **Title** | Required, up to 200 characters. |
| **Description** | Optional. |

Under **More options**, **Follows a maintenance** links it to one of the twenty last completed or cancelled maintenances, and **Save as a template** keeps it as a [template](incidents.md#templates).

**Publish the announcement** shows it at once and sends it to the [alert channels](alerts.md) that receive announcements.

## After a maintenance

On a completed maintenance, **Publish what is new** opens an announcement already filled in: the title "What is new after …", the category **What's new**, the same services, and the maintenance it follows.

## Updates

An announcement has no progress and never closes. Updates can still be added to it, as messages, and editors can always change it.

## How long it shows

An announcement never expires. It stays in the events list, in the **Recent activity** block and in the feed until newer events push it out: the block shows the ten latest, the feed the fifty latest. It never appears in the status banner, and it does not count in the number of new events since a member's last visit.
