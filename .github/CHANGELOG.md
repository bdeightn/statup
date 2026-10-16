# Changelog

All notable changes to Statup are listed here. The project is pre-v1: until 1.0, a minor version may change the interface or the database, and the README asks you to back up before upgrading.

## Unreleased

- Administrators arrange the dashboard from the dashboard itself: an "Arrange" button sits beside "New event", turns the page title into "Arrange the blocks" and gives way to "Done" in the same place; Escape also leaves. Not offered on a phone, where every block takes the full width.
- Each action has one way in and one name: the settings no longer repeat "Arrange the blocks", the icons page opens from the Services page (where editors, who may use it, can reach it), the page that sends to Teams, Slack or email is called "Alerts" and each of its entries a "channel", so "Notifications" now only means the bell, and adding a service reads "New service" everywhere. An event's progress is called "Progress" on every screen, and its side panel links to "Open the page".
- A saved event template can be removed from the "Start from a template" menu. A closed event tells editors that only an administrator may still change it, and a service with events says why it cannot be deleted. Texts that name a settings page now link to it.
- The general settings (instance name, logo, time zone, who can see the page) are saved together with one "Save" button, and leaving the page with unsaved changes asks first; choosing "Everyone" no longer opens the page on a single click.
- An event opened from a service's side panel leads back to that service, and the side panel of an event offers the step that follows it ("Write the report" once an incident is resolved, "Publish what's new" after a maintenance), taken on the event's page.
- Fixed: an update posted from an event's own page now confirms it is published, as a new event already did.

## 0.5.0, 2026-10-14

- An incident can say when its next update is due: when declaring it or posting an update, write a delay ("30 min", "1 h") or a time ("14:30"). Its page and side panel say it after the age of the last update ("last update 12 min ago, next due at 14:30"), and the status banner gives it its own line under the incident ("update due at 14:30", then "update due 20 min ago" once the time has passed without news). A service's state reads larger in the banner. A delay is offered again with the following update, so a steady rhythm needs no retyping; resolving the incident drops it. The database is upgraded on start.
- The library that cleans the text of updates moves to its latest major version, which fixes two flaws in options Statup never turns on (SVG animations and style attributes).
- The library that encrypts Statup's outgoing connections (notifications, checks) moves to its latest patch, which fixes four flaws reported on 14 October.
- Editors and administrators have a bell in the masthead: a count of unread notices, the latest ones under it, and a page that keeps them 30 days. It tells the member who announced an incident's next update when that update is 5 minutes late (also by email, once, with a mail server set), every editor when the checks find a service down, and the administrators when a destination refuses its messages. Opening the bell marks them seen. The database is upgraded on start.
- Services that fall in the same minute, such as those of a restarting server, are told in one message per destination ("40 services are not answering", with their names) and their return in one more, instead of a message each ([#13](https://github.com/karl-cta/statup/issues/13)); the bell shows them as one notice. The webhook gains a `services` list; `service` keeps naming a service alone. The database is upgraded on start.
- Each monitored service sets how long it may stay without an answer before it shows an outage, from 1 to 60 minutes ([#13](https://github.com/karl-cta/statup/issues/13)): "Declare an outage after [3] min without an answer", on the form of a new or edited service, so a critical server alerts after a minute and a printer after fifteen. Existing services keep three minutes, and the first launch uses them too. The database is upgraded on start.
- Once an incident is resolved, an editor can write its report: what happened, why, and what changes so it does not happen again. The report heads the incident's page and its side panel, the history marks the incident "Resolved · report", the feed carries it, and it is posted once to the destinations that receive incidents (webhook: `reviewed`); a later correction is not sent again. The database is upgraded on start.
- Fixed: an event's details offer "Back to …" only to those who may use it; readers, visitors and editors of a closed incident no longer see a button that only led to a refusal.

## 0.4.0, 2026-10-09

- Editors choose the order of services: Change order, on the Services page, gives each row an up and a down arrow, and the order applies wherever services are listed. Existing services keep their alphabetical order, and a new service goes last; the database is upgraded on start to keep it.
- "Forgot your password?" on the sign-in page: with a mail server set, a member receives a link that works once, for one hour, and is signed in once they choose a new password; without one, the page says to ask an administrator. The answer never tells whether an address has an account. The database is upgraded on start.
- An open incident tells how long ago its last update was posted: "last update 2h ago" under how long it has been open, on its page and in its side panel, and the status banner gives the age of the latest update ("2h ago") instead of its time, so an old message never looks current.
- Services with a problem can come first ([#11](https://github.com/karl-cta/statup/issues/11)): in Change order, on the Services page, "Put services with a problem first" lists degraded, down and under-maintenance services ahead of the others, on the dashboard and the Services page; each goes back to its place once it is fine again. Off by default.

## 0.3.1, 2026-10-08

- The browser tab icon sits on a white rounded tile, so it stays readable on a dark tab bar.
- A long service name in the Services block wraps onto a second line instead of being cut short, and the state moves below it when the column is narrow ([#10](https://github.com/karl-cta/statup/issues/10)).
- Fixed: in Safari, the "Show" title of a block's settings menu no longer sits on the menu's top border.

## 0.3.0, 2026-10-07

- Statup speaks German and Spanish, next to French and English: the language button opens the list of languages, and a member's choice is kept for their next sign-in.
- The time shows on 24 hours in French, German and Spanish, on 12 hours in English.
- `DEFAULT_LOCALE` accepts `de` and `es`.
- The database is upgraded on start so that a member's preferred language can be any of them.
- Adding a language takes its file in `locales/` and one line in the code; the contributing guide explains how.
- Switching language keeps your place on the page, and the current language is ticked in the list.
- Editors open a service's settings from a pencil at the end of its row in Services, and at the top of its side panel.
- The Services block of the dashboard shows ninety days of history when it has a row of its own, and thirty in a narrower column.
- The Services page shows each service's last ninety days under its row, with how many had an incident; a phone shows the last thirty.
- On a phone, the event list keeps the search in view and folds the other filters behind a Filters button, which counts the filters set.
- Statup posts each incident, maintenance and announcement where people already read: Microsoft Teams, Slack, Google Chat, Discord, Mattermost, a phone through ntfy, email, or any tool through a webhook. Administrators add the destinations under Settings, Notifications, choose what each one receives and in which language, and send a test before saving.
- A destination can also receive the outages the checks detect, and their end, for the team that runs the services.
- A destination that does not answer is tried again for about forty minutes, its messages in order; the Notifications page shows the last message each destination received, or why it failed.
- Email goes through your mail server, set with `SMTP_HOST`, `SMTP_SECURITY`, `SMTP_PORT`, `SMTP_USERNAME`, `SMTP_PASSWORD` and `SMTP_FROM`; the messages of one event stay in one conversation.
- Any other tool receives a versioned JSON body, described in the self-hosting guide.
- The database is upgraded on start to keep the destinations and their messages.
- Fixed: in the dark theme, the bar under a long form no longer shows as a darker band.
- Fixed: in the banner, a word in bold or italics no longer comes apart from the punctuation after it.
- Settings opens on a strip of shortcuts to the team, the notifications, the icons, the dashboard and the version, so they show without scrolling; General follows below.

## 0.2.1, 2026-10-06

- Fixed: in Safari with the dark theme, the page no longer darkens for a moment on each new page, and the toolbar keeps its colour.
- Pages now change at once instead of fading into each other; the first launch keeps its step transitions.
- In the dark theme, the masthead takes the colour of the page, as it does in the light theme.
- Fixed: in Safari, opening the account menu no longer scrolls the page back to its top.
- In the footer, the mark and the separator line up with the text beside them.
- The footer links to the page that explains how to get notified, so visitors find it even when the Recent activity block is hidden.
- Fixed: with several services disrupted, the banner gives the latest update of each incident on its own row, on one line, instead of a single update at the bottom that named no incident.

## 0.2.0, 2026-10-05

- Statup monitors a service on its own: in its form, under Monitoring, choose Web for an address that should answer, or Port for a server, a router or a firewall by its host and port, and test it before saving; the test says why an address does not answer.
- At the first launch, a service typed in can be monitored right away, from a fold under its name, and gets a built-in icon of its kind; the suggestions no longer include Google Workspace, Sage, Confluence and SharePoint.
- After three failed checks in a row, the service shows an outage marked "detected automatically", and comes back on its own as soon as it answers; nothing is declared during a maintenance that takes it down, or when every service fails at once.
- An editor declares the incident of a detected outage in one click, from the banner or the Services page, with the service, the impact and the start already filled in.
- A detected outage of fifteen minutes or more counts in the thirty days of the service; choosing None or changing the address of a service in a false outage clears it, from its state and from its thirty days.
- The Services page shows the last latency of each monitored service, or that its last check got no answer, in place of a label.
- `MONITORING=false` turns every check off.
- The database is upgraded on start to keep the checks of the services and the outages they detect.
- The web framework, the templates, the database layer and the sign-in sessions run on their current major versions; nothing changes on screen and nobody has to sign in again.

## 0.1.3, 2026-10-04

- On a phone, the masthead tucks away while you read down the page and comes back as soon as you scroll up.
- On a phone, the menu opens over the page without moving it, its pages are rows you can tap across the whole width, and signing out is a button of its own.
- An editor who tries to change a closed event is now refused with an access denied answer (403) that says only an administrator can change it, instead of a bad request answer.
- The libraries Statup is built on are updated to their latest compatible releases, with their bug and security fixes.

## 0.1.2, 2026-10-03

- On a page open to everyone, the sign-in page leads back to the dashboard, so a visitor who opened it out of curiosity is not stuck there.
- On a phone, tapping a service or an event opens its page instead of a panel over the whole screen.
- On a phone, the masthead scrolls away with the page instead of staying pinned over it.
- On a phone, the open menu fills the screen, its clock at the foot, instead of leaving the page showing under it.
- On a phone, the icon choice of a service fits the screen: the grid takes as many columns as the width holds.
- Fixed: filtering the events list by service showed an error page.

## 0.1.1, 2026-10-02

- The Compose file runs the published image instead of building Statup: installing takes seconds, and upgrading is `docker compose pull`.
- The Compose file opens Statup to the network again, so an install on a server is reached from every workstation right away.
- The first launch speaks to whoever runs a service, for colleagues or customers: its texts no longer assume an IT team, and the choice of who can read the page explains that an open page is how you tell your customers.
- The masthead tabs show where you are more clearly: the current tab's line sits under its word and slides to the next tab as you change page, and a hovered tab draws a lighter line instead of looking current.

## 0.1.0, 2026-09-29

First public version. Services with their state and thirty days of history; incidents, maintenances and announcements with dated updates and templates; an Atom feed; reader, editor and administrator roles; a public or members-only page; French and English; light and dark themes; a single binary with an embedded SQLite database, a Docker image and a Compose file.

Since the first public commit:

- A first launch in four steps beside a live preview of the page.
- One dashboard layout for everyone, arranged on the page itself: blocks are dragged, sized in quarters, hidden, and the services and activity blocks choose what they show.
- Incidents start at the step you choose; maintenance can leave its services up; announcements read as an article; kind labels carry the impact ("Major incident") or the category.
- Administrators give a member a new temporary password from the Team page.
- Settings laid out as one row per setting; the services page names the event that holds a service in its state.
- Page transitions and a short highlight on what just arrived or changed, both off with reduced motion.
- Fixed: publishing needed two clicks in Safari; switching language right after a refused form could land on an error page; three menus kept the light theme's shadow in the dark theme; the Docker image and the release archive left out the first launch's transitions.
- Passwords chosen from now on need 12 characters with upper and lower case letters, a digit and a symbol, or 20 characters of any kind.
- The Compose file publishes Statup on this machine only until you open it, so nobody else can create the first administrator.
- Administrators see the version in Settings, and the newer one once it is published: the server asks GitHub once a day (`UPDATE_CHECK=false` turns it off).
- Security: behind a proxy, the client address is read from the one header named in `CLIENT_IP_HEADER` (the last entry of `X-Forwarded-For` by default); an IPv6 client is limited as its whole /64; a sign-in form stays valid one hour instead of seven days; wrong current passwords are limited on the profile page; a temporary password is kept nowhere in clear and expires after seven days; past 64 password checks waiting, a sign-in is turned away instead of queued.
- A Docker image for x86-64 and ARM servers, published with each version to the GitHub Container Registry.

Database: four migrations, applied on start (a maintenance without downtime flag, a single dashboard layout that keeps the members' arrangement, the expiry of temporary passwords, and the layout table without its unused user column).
