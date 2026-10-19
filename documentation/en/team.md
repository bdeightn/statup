---
title: Team and roles
description: "Readers, editors and administrators, how their accounts are made, and who can see the page."
---

## Roles

| Role | Can |
|---|---|
| **Reader** | Read the dashboard, the events and the feed, and edit their own profile. |
| **Editor** | Everything a reader can, plus publish incidents, maintenance and announcements, set service states, and manage services, templates and icons. |
| **Administrator** | Everything, plus the settings, the team, the alerts, the Journal, the layout of the dashboard, and changing or deleting closed events. |

## Who can see the page

Chosen at the [first launch](install.md#first-launch), then in **Settings**, under **Who can see the page**:

- **Members only**: signing in is required, and feed readers can no longer read the feed.
- **Everyone**: visitors read the page and the feed without an account.

## Add a member

Accounts are created by an administrator: anyone else can only sign up on an empty instance, to become its first administrator.

**Settings**, **Team**, then **Add a member**: **Name**, **Email address** and **Role**, **Reader** by default. **Create the account** shows a temporary password once, with **Copy**.

- No email is sent: hand the password over yourself. It is shown only this once and kept nowhere.
- It works for seven days. At the first sign-in, the member is asked to choose their own, on **Choose your password**.
- An address already used by another account, disabled ones included, is refused.

## Password rule

Every chosen password needs 12 characters with upper and lower case letters, a digit and a symbol, or at least 20 characters of any kind. Passwords are hashed with Argon2id.

## Change a role

Each member's row has a role menu, and the change asks to be confirmed. Nobody can change their own role, and the last administrator cannot become something else: there is always at least one active administrator.

## Give a new password

**New password**, then **Generate**, gives an active member a new temporary password, valid seven days. The old one stops working and their open sessions are signed out. For your own password, use **My profile**. When nobody can sign in any more, the host can do it from the [command line](command-line.md).

## Disable a member

**Disable** stops a member from signing in; their open sessions end at their next click. **Enable** lets them back in. Their address stays reserved meanwhile. Nobody can disable themselves, nor the last active administrator.

Members cannot be deleted: disable them instead.

## My profile

Every member reaches **My profile** from their menu:

- **Name and address**: their name and email address.
- **Password**: the current one, then the new one twice. The other sessions of the account are signed out.

The language is chosen from the language switch, and is kept on the account. The light or dark theme is kept by each browser.

## Sessions

Ticking **Stay signed in for 30 days** at sign-in keeps the session 30 days without a visit; otherwise it ends after 24 hours.
