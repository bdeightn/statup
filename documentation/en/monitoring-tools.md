---
title: Monitoring tools
description: "Let Zabbix, Prometheus, Grafana, Uptime Kuma or a script tell Statup that a service is down or back up."
---

A monitoring tool already watches your servers. Set it once to tell Statup when a service goes down and when it comes back, and Statup shows it as it shows an outage its own [checks](monitoring.md) find.

## How a report works

Every tool posts to one address, `/api/v1/reports` on the address of your page, with an [API key](api.md#keys): make one per tool, named after it, so its reports are signed with its name.

- The service is named as in Statup, by its name or its slug, whatever the case. Name the tool's monitor, label or host the same way. Two services of the same name: the first in the order of the Services page takes the report, so name the other by its slug.
- **Down**: the service shows **Outage**, marked **Detected automatically**, from the moment the report arrives. The channels that receive **Outages detected automatically** are told, and editors find **Declare an incident** with the incident filled in. Statup never publishes an incident itself.
- **Up**: the outage ends and the state set by hand comes back.
- A service that an open incident covers sends no alert. Services reported down together in one message are told in one alert per channel.

A report is ignored, and a line written in the [Journal](journal.md), for a service Statup does not know, a service Statup checks itself (its own checks would undo the report the next minute), and a service reported down during a maintenance under way that names it.

## Statup's format

A script, or a tool whose message you write, sends the name, the state and an optional message, in JSON or as a form:

```bash
curl https://status.example.com/api/v1/reports \
  -H "Authorization: Bearer $STATUP_KEY" \
  -H "Content-Type: application/json" \
  -d '{"service": "Mail", "state": "down", "message": "SMTP does not answer"}'
```

- `service`: a name or a slug.
- `state`: `down` or `up`, whatever the case.
- `message`: shown in the Journal line.

A tool that cannot set a header sends the key as a `key` field of the body instead, never in the address:

```bash
curl https://status.example.com/api/v1/reports \
  --data-urlencode "key=$STATUP_KEY" \
  --data-urlencode "service=Mail" \
  --data-urlencode "state=up"
```

## The answer

A report answers 200 with what it changed and what it left:

```json
{"applied": [{"service": "Mail", "state": "down"}], "ignored": [{"service": "VPN", "reason": "unchanged"}]}
```

| `reason` | |
|---|---|
| `unchanged` | The service is already down, or already up. |
| `unknown_service` | No service has this name or slug. |
| `monitored_by_statup` | Statup checks this service itself. |
| `under_maintenance` | A maintenance under way covers the service. |
| `no_service_label` | An Alertmanager or Grafana alert without a `service` label. |
| `no_monitor_name` | An Uptime Kuma message without its monitor. |
| `not_down_or_up` | A state that is neither, such as pending. |

In Statup's format, the three refusals answer an error instead, so a script sees it failed: 404 `unknown_service`, 409 `monitored_by_statup`, 409 `under_maintenance`. A missing name answers 400 `service_required`, a state other than `down` or `up` 400 `invalid_state`, a body in no format Statup reads 400 `unknown_format`. Without a valid key, the answer is 401; a body Statup cannot read answers 400 `invalid_data`, even when it holds the key. See [API](api.md#errors) for the others.

## Tools that send their own format

Statup reads the message of these tools as they send it.

### Uptime Kuma

1. In **Settings**, **Notifications**, **Setup Notification**, choose **Webhook**.
2. **Post URL**: `https://status.example.com/api/v1/reports`.
3. **Request Body**: keep **Preset - application/json**. The multipart preset is refused, and a custom body goes out as a form, which Statup does not read as JSON.
4. Turn on **Additional Headers** and write:

```json
{"Authorization": "Bearer stp_your_key"}
```

5. Turn the notification on for each monitor whose name is a service in Statup.

The notification's **Test** answers without changing anything. A monitor that is pending or in maintenance changes nothing either.

### Prometheus

Prometheus sends its alerts through Alertmanager, its alert manager: Statup reads what Alertmanager sends. Give each alert rule a `service` label that names the service:

```yaml
groups:
  - name: services
    rules:
      - alert: MailDown
        expr: probe_success{job="mail"} == 0
        for: 2m
        labels:
          service: Mail
        annotations:
          summary: SMTP does not answer
```

Then send those alerts to Statup, with the key in a file only Alertmanager reads:

```yaml
receivers:
  - name: statup
    webhook_configs:
      - url: https://status.example.com/api/v1/reports
        http_config:
          authorization:
            credentials_file: /etc/alertmanager/statup-key
```

Resolved alerts are sent by default, and bring the service back up. `summary` goes in the Journal line, else the alert's name. An alert without a `service` label, such as the Watchdog, is left out. A body is 64 KB at most: group the alerts by `service` in the route rather than sending hundreds at once. Alertmanager repeats an alert every four hours by default: a label that names no service leaves a Journal line each time, until it is fixed.

### Grafana

1. In each alert rule, under its labels, add `service` with the name of the service.
2. In **Alerting**, **Contact points**, add a contact point with the integration **Webhook** and the URL `https://status.example.com/api/v1/reports`.
3. In its optional settings, set the authorization header scheme to `Bearer` and its credentials to the key. Keep the URL without the key: Grafana stores it in plain text.
4. Route the rules' notifications to this contact point.

Grafana sends resolved alerts by default; keep it so, or services never come back up.

## Tools with a message you write

Zabbix posts a body you write: write it in Statup's format.

### Zabbix

In **Alerts**, **Media types**, create a media type of type **Webhook**, with these parameters:

| Name | Value |
|---|---|
| `url` | `https://status.example.com/api/v1/reports` |
| `key` | Your key |
| `service` | `{HOST.NAME}` |
| `value` | `{EVENT.VALUE}` |
| `update` | `{EVENT.UPDATE.STATUS}` |
| `problem` | `{EVENT.NAME}` |

and this script:

```text
var params = JSON.parse(value);
if (params.update === '1') {
    return 'Not a problem nor a recovery';
}
var request = new HttpRequest();
request.addHeader('Authorization: Bearer ' + params.key);
request.addHeader('Content-Type: application/json');
request.post(params.url, JSON.stringify({
    service: params.service,
    state: params.value === '1' ? 'down' : 'up',
    message: params.problem
}));
if (request.getStatus() !== 200) {
    throw 'Statup answered ' + request.getStatus();
}
return 'OK';
```

Give the media type a message template for **Problem** and one for **Problem recovery**, then, in the action that should tell Statup, send the problem with this media type and add a **Recovery operation** that sends it too. Here the service is named after the host: write another macro in `service` to name it otherwise. `{EVENT.UPDATE.STATUS}` keeps an acknowledgement from being taken for an outage.

## Any other tool

The tools above are the ones whose steps were tried on real instances. Any other tool reaches Statup the same way, in Statup's format.

### A tool that calls a web address

If it can post to an address when an alert starts and when it ends, give it the address `https://status.example.com/api/v1/reports`, the header `Authorization: Bearer` followed by the key, and a body in [Statup's format](#statups-format): the name of the service, `down` or `up`, and a message if you like. A tool that can set no header puts the key in a `key` field of the body, sent as a form.

### A tool that runs a script

If it runs a command on each notification, as Nagios and its relatives do, this script tells Statup. Save the key's header in a file only the monitoring user reads, so the key never shows in the list of processes:

```bash
printf 'Authorization: Bearer %s\n' "stp_your_key" > /etc/statup/header
chmod 600 /etc/statup/header
```

Then save this script as `/usr/local/bin/statup-report`, executable:

```bash
#!/bin/sh
# Tells Statup that a service is down or back up.
# Usage: statup-report <service> <PROBLEM|RECOVERY> [message]
case "$2" in
    PROBLEM) state=down ;;
    RECOVERY) state=up ;;
    *) exit 0 ;;
esac
curl -fsS --max-time 10 https://status.example.com/api/v1/reports \
    -H @/etc/statup/header \
    --data-urlencode "service=$1" \
    --data-urlencode "state=$state" \
    --data-urlencode "message=$3"
```

Have the tool call it with the name of the service, `PROBLEM` or `RECOVERY`, and a message: any other second word leaves Statup as it is. A cron job can call it the same way.
