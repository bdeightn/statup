---
title: Maintenance
description: "Announce planned work ahead, let it start and end on its own, or start it right away."
---

A maintenance tells people that work is planned or under way, on which services, and until when. Editors and administrators publish one from **New event**, with **Type** set to **Maintenance**.

## Announce a maintenance

| Field | Description |
|---|---|
| **Start** | **Now** by default. Pick a day and a **Start time** to announce it ahead: the list offers every half hour, and any time can be typed. |
| **Duration** | In hours and minutes. The **Estimated end** follows, such as "Tomorrow at 02:00". Without a duration: "No set end". |
| **Services affected** | The services the work touches. |
| **Title** | Required. Until you type one, it writes itself, such as "Maintenance of Email". |
| **Description** | Optional. |

Times are in the instance's time zone, which the form shows. Under **More options**, **No downtime** keeps the services usable during the work: their state never changes. It can only be set when the maintenance is created.

**Announce the maintenance** publishes it and sends it to the [alert channels](alerts.md) that receive maintenance.

## Ahead, or right away

- **With a start**, the maintenance is announced: **Scheduled**. Its services do not change until it starts.
- **Without one**, it begins at once: **Under way**.

## It starts and ends on its own

Every minute, Statup starts each scheduled maintenance whose start has come, and completes each maintenance under way whose end has come. Both are recorded at the times announced, and each sends its alert.

A maintenance without a duration never ends on its own: complete it by hand. A start or an end made on its own cannot be undone.

## By hand

In **Post an update**, choose:

- **Under way** to start it now;
- **Completed** to end it now, earlier than planned;
- **Cancelled** to call it off, before or during the work.

## Move it

While it is scheduled, **Edit** changes its start or its duration, and the alert channels receive the new window ("Moved to …"). Once it has started, its start is locked; its duration can still change.

## What the services show

While it is under way, the services it names show **Under maintenance**, unless it is marked **No downtime**, or unless something more serious holds them. A scheduled maintenance changes nothing yet.

[Monitoring](monitoring.md#when-no-outage-is-declared) declares no outage for a service under maintenance.

## On the page

- When nothing is disrupted, the banner says "Maintenance in progress" during the work, and "Maintenance planned for …" when one starts within seven days.
- The **Maintenance** block lists those under way, then the scheduled ones by start, then the five last completed or cancelled.
- The maintenance's page says "in 2d" before it starts and "started 20 min ago" during it, with the planned start and end on its timeline.

## After the work

Once a maintenance is completed, **Publish what is new** opens an announcement already filled in: "What is new after …", in the category **What's new**, on the same services, following that maintenance. See [Announcements](announcements.md).

## Edit or delete

The rules are those of [incidents](incidents.md#edit-or-delete): once a maintenance is **Completed** or **Cancelled**, only an administrator can still change or delete it.
