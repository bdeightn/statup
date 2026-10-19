---
title: Services
description: "The tools people rely on, their state, how it changes on its own, and their availability history."
---

A service is a tool people rely on every day: mail, the VPN, the intranet, the phones, a website. Each one shows a state on the page, and the events that name it.

Editors and administrators manage services from the **Services** page. Readers and visitors see them on the dashboard, where a service opens in a side panel with its state, its last 30 days and its latest events.

## Create a service

**New service**, on the Services page, opens the form:

| Field | Description |
|---|---|
| **Name** | Required, up to 100 characters. Two services may share a name. |
| **Description** | Optional, one sentence on what the service is for, up to 500 characters. |
| **Monitoring** | **None**, **Web** or **Port**: see [Monitoring](monitoring.md). |
| **Icon** | Optional: a built-in icon, or one you upload. |

**Create the service** adds it at the end of the list, in the state **Operational**. Administrators can also add services during the [first launch](install.md#first-launch).

To change a service later, open it by its name or by its pencil on the Services page, then **Save**. Its state is not in the form: it is set from the list.

## Icons

- 24 built-in icons, from Server to Printing.
- **Upload an icon**: PNG, JPG, WebP or SVG, up to 256 KB. Images larger than 128 pixels are reduced; an SVG is cleaned of anything that could run or load something.
- Uploaded icons form one library, shared by services, events and templates, under **Icons** on the Services page. An icon still in use cannot be deleted.

## States

| State | Set by hand | Set on its own |
|---|---|---|
| **Operational** | Yes | |
| **Degraded** | Yes | By an open incident of impact **Minor**, or of no impact |
| **Outage** | Yes | By an open incident of impact **Major**, or by [monitoring](monitoring.md) |
| **Under maintenance** | Yes | By a maintenance under way, unless it is marked **No downtime** |

### Set a state by hand

On the Services page, each service has a state menu. A choice is published at once, with **Undo** beside it. **Outage** first asks "Show “Outage” to every visitor?", then **Publish**.

### When a state changes on its own

An incident sets the state of the services it names while it is **Investigating** or **Fixing**. A maintenance does so while it is **Under way**. Announcements never do. See [Incidents](incidents.md) and [Maintenance](maintenance.md).

When several things apply, the service shows the most serious: **Outage**, then **Degraded**, then **Under maintenance**, then **Operational**. A maintenance under way does not hide a detected outage, and a state set by hand to **Outage** stays during a minor incident.

When the incident moves to **Monitoring**, is resolved or cancelled, or the maintenance ends, the service goes back to the most serious of what is left, which is usually the state set by hand.

While an event or a detected outage holds a service, its menu gives way to the state shown, with a link to that event, or **Detected automatically** and **Declare an incident**. The menu comes back when it ends.

## Order

**Change order** puts an up and a down arrow on each service. The order is saved a moment after the last move, and applies wherever services are listed, the dashboard included. **Done** or Escape leaves the mode.

In the same mode, **Put services with a problem first** lists every service that is not **Operational** before the others, each group in the chosen order. It is off by default.

## Availability history

Each service has one bar per day, in the instance's time zone, ending today. The dashboard and the Services page show 90 days where there is room and the last 30 on a narrow screen; the side panel always shows 30.

| Bar | When |
|---|---|
| Green | No incident that day |
| Amber | A **Minor** incident or one of no impact, or a detected outage of fifteen minutes or more |
| Red | A **Major** incident |
| Grey | Before the service existed |

- The most serious incident of the day wins. An incident counts from its start until it moves to **Monitoring**, or until it ends; a cancelled incident never counts.
- Maintenance never colours a day.
- A detected outage colours a day only when no incident covers it, and only when it lasts fifteen minutes or more in total.

There is no percentage: the caption counts the days with an incident.

## Delete a service

A service that no event ever named can be deleted from its form, under **Delete the service**. It leaves without a trace, its detected outages included.

A service that events name is kept, so their history stays whole. To stop showing it on the dashboard, an administrator leaves it out in **Arrange**: see [Settings](settings.md#arrange-the-dashboard). A service left out comes back as soon as it is not **Operational**.

## Who can do what

| | Reader | Editor | Administrator |
|---|---|---|---|
| See the states and the history, open a service's side panel | Yes | Yes | Yes |
| Create, edit, set a state, change the order, delete, manage icons | | Yes | Yes |
| Leave a service out of the dashboard | | | Yes |
