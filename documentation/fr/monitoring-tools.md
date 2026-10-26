---
title: Outils de supervision
description: "Laisser Zabbix, Prometheus, Grafana, Uptime Kuma ou un script prévenir Statup qu'un service tombe ou revient."
---

Un outil de supervision surveille déjà vos serveurs. Réglez-le une fois pour qu'il prévienne Statup quand un service tombe et quand il revient : Statup l'affiche comme une panne que sa propre [surveillance](monitoring.md) aurait trouvée.

## Comment marche un signalement

Chaque outil envoie à une seule adresse, `/api/v1/reports` sur l'adresse de votre page, avec une [clé d'API](api.md#keys) : créez-en une par outil, à son nom, pour que ses signalements soient signés de ce nom.

- Le service est nommé comme dans Statup, par son nom ou son identifiant d'adresse (slug), sans tenir compte de la casse. Nommez de la même façon la sonde, l'étiquette ou l'hôte dans l'outil. Deux services du même nom : le premier dans l'ordre de la page Services reçoit le signalement ; nommez l'autre par son slug.
- **En panne** : le service affiche **En panne**, marqué **Détecté automatiquement**, à partir de l'arrivée du signalement. Les canaux qui reçoivent les **Pannes détectées automatiquement** sont prévenus, et les éditeurs trouvent **Déclarer un incident** avec l'incident prérempli. Statup ne publie jamais d'incident lui-même.
- **Rétabli** : la panne se termine et l'état réglé à la main revient.
- Un service couvert par un incident ouvert n'envoie pas d'alerte. Les services signalés en panne ensemble dans un même message partent dans une seule alerte par canal.

Un signalement est ignoré, et une ligne écrite dans le [Journal](journal.md), pour un service que Statup ne connaît pas, un service que Statup vérifie lui-même (sa propre vérification annulerait le signalement la minute suivante), et un service signalé en panne pendant une maintenance en cours qui le cite.

## Le format de Statup

Un script, ou un outil dont vous écrivez le message, envoie le nom, l'état et un message facultatif, en JSON ou en formulaire :

```bash
curl https://status.example.com/api/v1/reports \
  -H "Authorization: Bearer $STATUP_KEY" \
  -H "Content-Type: application/json" \
  -d '{"service": "Mail", "state": "down", "message": "SMTP does not answer"}'
```

- `service` : un nom ou un slug.
- `state` : `down` ou `up`, sans tenir compte de la casse.
- `message` : affiché dans la ligne du Journal.

Un outil qui ne sait pas poser d'en-tête envoie la clé dans un champ `key` du corps, jamais dans l'adresse :

```bash
curl https://status.example.com/api/v1/reports \
  --data-urlencode "key=$STATUP_KEY" \
  --data-urlencode "service=Mail" \
  --data-urlencode "state=up"
```

## La réponse

Un signalement répond 200 avec ce qu'il a changé et ce qu'il a laissé :

```json
{"applied": [{"service": "Mail", "state": "down"}], "ignored": [{"service": "VPN", "reason": "unchanged"}]}
```

| `reason` | |
|---|---|
| `unchanged` | Le service est déjà en panne, ou déjà rétabli. |
| `unknown_service` | Aucun service n'a ce nom ni ce slug. |
| `monitored_by_statup` | Statup vérifie ce service lui-même. |
| `under_maintenance` | Une maintenance en cours couvre le service. |
| `no_service_label` | Une alerte d'Alertmanager ou de Grafana sans étiquette `service`. |
| `no_monitor_name` | Un message d'Uptime Kuma sans sa sonde. |
| `not_down_or_up` | Un état qui n'est ni l'un ni l'autre, comme en attente. |

Dans le format de Statup, les trois refus répondent une erreur à la place, pour qu'un script voie l'échec : 404 `unknown_service`, 409 `monitored_by_statup`, 409 `under_maintenance`. Un nom absent répond 400 `service_required`, un état autre que `down` ou `up` 400 `invalid_state`, un corps dans aucun format que Statup lit 400 `unknown_format`. Sans clé valable, la réponse est 401 ; un corps que Statup ne peut pas lire répond 400 `invalid_data`, même s'il contient la clé. Voir [API](api.md#errors) pour les autres.

## Les outils qui envoient leur propre format

Statup lit le message de ces outils tel qu'ils l'envoient.

### Uptime Kuma

1. Dans **Settings**, **Notifications**, **Setup Notification**, choisissez **Webhook**.
2. **Post URL** : `https://status.example.com/api/v1/reports`.
3. **Request Body** : gardez **Preset - application/json**. Le format multipart est refusé, et un corps personnalisé part en formulaire, que Statup ne lit pas comme du JSON.
4. Activez **Additional Headers** et écrivez :

```json
{"Authorization": "Bearer stp_your_key"}
```

5. Activez la notification sur chaque sonde dont le nom est un service de Statup.

Le **Test** de la notification répond sans rien changer. Une sonde en attente ou en maintenance ne change rien non plus.

### Prometheus Alertmanager

Donnez à chaque règle d'alerte une étiquette `service` qui nomme le service :

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

Puis envoyez ces alertes à Statup, avec la clé dans un fichier que seul Alertmanager lit :

```yaml
receivers:
  - name: statup
    webhook_configs:
      - url: https://status.example.com/api/v1/reports
        http_config:
          authorization:
            credentials_file: /etc/alertmanager/statup-key
```

Les alertes résolues sont envoyées par défaut, et rétablissent le service. `summary` va dans la ligne du Journal, sinon le nom de l'alerte. Une alerte sans étiquette `service`, comme le Watchdog, est laissée de côté. Un corps fait 64 Ko au plus : groupez les alertes par `service` dans la route plutôt que d'en envoyer des centaines d'un coup. Alertmanager répète une alerte toutes les quatre heures par défaut : une étiquette qui ne nomme aucun service laisse une ligne au Journal à chaque fois, jusqu'à ce qu'elle soit corrigée.

### Grafana

1. Dans chaque règle d'alerte, parmi ses étiquettes, ajoutez `service` avec le nom du service.
2. Dans **Alerting**, **Contact points**, ajoutez un point de contact avec l'intégration **Webhook** et l'URL `https://status.example.com/api/v1/reports`.
3. Dans ses réglages facultatifs, mettez le schéma de l'en-tête d'autorisation à `Bearer` et ses identifiants à la clé. Gardez l'URL sans la clé : Grafana la stocke en clair.
4. Envoyez les notifications des règles vers ce point de contact.

Grafana envoie les alertes résolues par défaut ; gardez-le ainsi, sinon les services ne sont jamais rétablis.

## Les outils dont vous écrivez le message

Ces outils envoient un corps que vous écrivez : écrivez-le dans le format de Statup.

### Zabbix

Dans **Alerts**, **Media types**, créez un type de média de type **Webhook**, avec ces paramètres :

| Nom | Valeur |
|---|---|
| `url` | `https://status.example.com/api/v1/reports` |
| `key` | Votre clé |
| `service` | `{HOST.NAME}` |
| `value` | `{EVENT.VALUE}` |
| `update` | `{EVENT.UPDATE.STATUS}` |
| `problem` | `{EVENT.NAME}` |

et ce script :

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

Donnez au type de média un modèle de message pour **Problem** et un pour **Problem recovery**, puis, dans l'action qui doit prévenir Statup, envoyez le problème par ce type de média et ajoutez une **Recovery operation** qui l'envoie aussi. Ici, le service porte le nom de l'hôte : mettez une autre macro dans `service` pour le nommer autrement. `{EVENT.UPDATE.STATUS}` évite qu'un acquittement soit pris pour une panne.

### UptimeRobot

Sur une offre qui a les webhooks, dans **Integrations**, ajoutez un **Webhook** :

1. L'URL `https://status.example.com/api/v1/reports`.
2. Un en-tête personnalisé `Authorization` avec la valeur `Bearer` suivie de la clé.
3. Un corps POST personnalisé, envoyé en JSON :

```json
{"service": "*monitorFriendlyName*", "state": "*alertTypeFriendlyName*", "message": "*alertDetails*"}
```

4. Les événements **Down** et **Up**.

UptimeRobot envoie chacun une seule fois, sans nouvel essai.

### Datadog

Dans l'intégration **Webhooks**, ajoutez deux webhooks avec l'URL `https://status.example.com/api/v1/reports` et les en-têtes personnalisés :

```json
{"Authorization": "Bearer stp_your_key"}
```

Le premier, `statup-down`, avec ce corps, et le second, `statup-up`, avec `up` pour état :

```json
{"service": "$TAGS[service]", "state": "down", "message": "$EVENT_TITLE"}
```

Donnez à chaque moniteur un tag `service`, et appelez les webhooks depuis son message :

```text
{{#is_alert}}@webhook-statup-down{{/is_alert}}
{{#is_recovery}}@webhook-statup-up{{/is_recovery}}
```

## Les outils qui ne savent pas poser d'en-tête

### PRTG

L'action HTTP de PRTG ne pose aucun en-tête et envoie son contenu en formulaire : la clé va dans le contenu.

1. Dans **Setup**, **Account Settings**, **Notification Templates**, ajoutez un modèle avec la méthode **Execute HTTP Action**, l'URL `https://status.example.com/api/v1/reports`, la méthode `POST`, et ce contenu :

```text
key=stp_your_key&service=%device&state=down&message=%message
```

2. Ajoutez un second modèle, le même avec `state=up`.
3. Sur l'appareil ou le capteur, ajoutez un déclencheur d'état : quand le capteur est **Down**, le premier modèle ; quand il ne l'est plus, le second.

Ici, le service porte le nom de l'appareil. Le manuel de PRTG ne dit pas s'il encode les variables : évitez `&`, `=` et `+` dans ces noms d'appareils.

## Les outils qui lancent un script

Nagios, Icinga, Centreon et Checkmk lancent une commande à chaque notification. Un seul script sert à tous. Enregistrez l'en-tête de la clé dans un fichier que seul l'utilisateur de la supervision lit, pour que la clé n'apparaisse jamais dans la liste des processus :

```bash
printf 'Authorization: Bearer %s\n' "stp_your_key" > /etc/statup/header
chmod 600 /etc/statup/header
```

Puis enregistrez ce script sous `/usr/local/bin/statup-report`, exécutable :

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

Les acquittements, les oscillations et les arrêts planifiés laissent Statup tel quel.

### Nagios et Centreon

Une commande de notification, dans Centreon depuis **Configuration**, **Commands** :

```text
define command {
    command_name    notify-statup
    command_line    /usr/local/bin/statup-report "$SERVICEDESC$" "$NOTIFICATIONTYPE$" "$SERVICEOUTPUT$"
}
```

Donnez-la à un contact dans `service_notification_commands`, avec `service_notification_options c,r` : les problèmes critiques et les rétablissements seulement, car un avertissement est aussi un problème. Un rétablissement n'est envoyé qu'après son problème.

### Icinga 2

```text
object NotificationCommand "statup" {
    command = [ "/usr/local/bin/statup-report", "$service.name$", "$notification.type$", "$service.output$" ]
}
```

Appliquez-la dans une `Notification` avec `types = [ Problem, Recovery ]`.

### Checkmk

Enregistrez ceci sous `~/local/share/check_mk/notifications/statup`, exécutable, puis choisissez **Statup** comme méthode d'une règle de notification :

```bash
#!/bin/sh
# Statup
exec /usr/local/bin/statup-report "${NOTIFY_SERVICEDESC:-$NOTIFY_HOSTNAME}" "$NOTIFY_NOTIFICATIONTYPE"
```

La notification d'un hôte nomme le service d'après l'hôte.
