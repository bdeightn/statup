---
title: Outils de supervision
description: "Laisser Zabbix, Prometheus, Grafana, Uptime Kuma ou un script prévenir Statup qu'un service tombe ou revient."
---

Un outil de supervision surveille déjà vos serveurs. Réglez-le une fois pour qu'il prévienne Statup quand un service tombe et quand il revient : Statup l'affiche comme une panne que sa propre [surveillance](monitoring.md) aurait trouvée.

## Comment marche un signalement

Chaque outil envoie à une seule adresse, `/api/v1/reports` sur l'adresse de votre page, avec une [clé d'API](api.md#keys) : créez-en une par outil, à son nom, pour que ses signalements soient signés de ce nom.

- Le service est nommé comme dans Statup, par son nom ou son identifiant d'adresse (slug), sans tenir compte de la casse. Nommez de la même façon la sonde, l'étiquette ou l'hôte dans l'outil. Deux services du même nom : le premier dans l'ordre de la page Services reçoit le signalement ; nommez l'autre par son slug.
- **Panne** : le service affiche **En panne**, marqué **Détecté automatiquement**, à partir de l'arrivée du signalement. Les canaux qui reçoivent les **Pannes détectées automatiquement** sont prévenus, et les éditeurs trouvent **Déclarer un incident** avec l'incident prérempli. Statup ne publie jamais d'incident lui-même.
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

Dans le format de Statup, les trois refus répondent une erreur à la place, pour qu'un script voie l'échec : 404 `unknown_service`, 409 `monitored_by_statup`, 409 `under_maintenance`. Un nom absent répond 400 `service_required`, un état autre que `down` ou `up` 400 `invalid_state`, un corps dans aucun format que Statup lit 400 `unknown_format`. Sans clé valable, la réponse est 401 et rien n'est écrit au Journal : le journal de l'outil montre le refus. Un corps que Statup ne peut pas lire répond 400 `invalid_data`, même s'il contient la clé. Parmi les raisons ci-dessus, `unknown_service`, `monitored_by_statup` et `under_maintenance` laissent aussi une ligne au Journal ; les autres non. Voir [API](api.md#errors) pour les autres.

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

### Prometheus

Prometheus envoie ses alertes par Alertmanager, son gestionnaire d'alertes : Statup lit ce qu'Alertmanager envoie. Donnez à chaque règle d'alerte une étiquette `service` qui nomme le service. `probe_success` vient d'une sonde du blackbox exporter ; pour une cible que Prometheus collecte lui-même, écrivez `up{job="mail"} == 0`.

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

Puis envoyez ces alertes à Statup, groupées par service, avec la clé dans un fichier. Dans une configuration qui a déjà ses destinataires, ajoutez Statup comme route fille de l'existante.

```yaml
route:
  receiver: statup
  group_by: [service]

receivers:
  - name: statup
    webhook_configs:
      - url: https://status.example.com/api/v1/reports
        http_config:
          authorization:
            credentials_file: /etc/alertmanager/statup-key
```

Le fichier de la clé doit être lisible par l'utilisateur sous lequel tourne Alertmanager, `65534` dans l'image officielle, et par personne d'autre ; sinon chaque message échoue avec `permission denied` dans le journal d'Alertmanager, alors qu'il démarre sans rien dire :

```bash
chown 65534 /etc/alertmanager/statup-key
chmod 400 /etc/alertmanager/statup-key
```

Les alertes résolues sont envoyées par défaut, et rétablissent le service. `summary` va dans la ligne du Journal de la panne, sinon le nom de l'alerte. Une alerte sans étiquette `service`, comme le Watchdog, est laissée de côté, sans ligne au Journal. Un corps fait 64 Ko au plus : grouper par `service` garde chacun petit. Alertmanager répète une alerte toutes les quatre heures par défaut : une étiquette qui ne nomme aucun service laisse une ligne au Journal à chaque fois, jusqu'à ce qu'elle soit corrigée. Avec une mauvaise clé, le journal d'Alertmanager affiche `unexpected status code 401`, et il réessaie au prochain intervalle de groupe.

### Grafana

1. Dans chaque règle d'alerte, sous **Add folder and labels**, ajoutez l'étiquette `service` avec le nom du service. Sous **Set evaluation behavior**, mettez **Alert state if no data or all values are null** et **Alert state if execution error or timeout** à **Keep Last State** : sinon Statup affiche une panne dès que Grafana ne peut plus lire sa source de données.
2. Dans **Alerting**, **Notification configuration**, onglet **Contact points**, ajoutez un point de contact avec l'intégration **Webhook** et l'URL `https://status.example.com/api/v1/reports`.
3. Dans **Optional Webhook settings**, laissez **Authorization Header - Scheme** à `Bearer` et mettez la clé dans **Authorization Header - Credentials**. Gardez l'URL sans la clé : Grafana la stocke en clair.
4. Envoyez vers ce point de contact les règles qui nomment un service : dans chaque règle, **Select contact point**, ou une politique de notification qui filtre `service =~ .+`. Les règles sans l'étiquette atteindraient Statup pour rien.

Grafana envoie les alertes résolues par défaut ; gardez-le ainsi, sinon les services ne sont jamais rétablis. Un service revient quand le message de résolution part, au **Group interval** de la politique, cinq minutes par défaut : raccourcissez-le dans ses **Timing options** si c'est trop lent. `summary` va dans la ligne du Journal de la panne. Le **Test** du point de contact ne change rien, sauf un test personnalisé qui porte une étiquette `service` : il met alors ce service en panne pour de vrai.

## Les outils dont vous écrivez le message

Zabbix envoie un corps que vous écrivez : écrivez-le dans le format de Statup.

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

## Tout autre outil

Les outils ci-dessus sont ceux dont la marche à suivre a été essayée sur de vraies installations. Tout autre outil joint Statup de la même façon, dans le format de Statup.

### Un outil qui appelle une adresse web

S'il sait envoyer un message à une adresse quand une alerte commence et quand elle se termine, donnez-lui l'adresse `https://status.example.com/api/v1/reports`, l'en-tête `Authorization: Bearer` suivi de la clé, et un corps dans le [format de Statup](#statups-format) : le nom du service, `down` ou `up`, et un message si vous voulez. Un outil qui ne sait poser aucun en-tête met la clé dans un champ `key` du corps, envoyé en formulaire.

### Un outil qui lance un script

S'il lance une commande à chaque notification, comme Nagios et ses cousins, ce script prévient Statup. Enregistrez l'en-tête de la clé dans un fichier que seul l'utilisateur de la supervision lit, pour que la clé n'apparaisse jamais dans la liste des processus :

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

Faites-le appeler par l'outil avec le nom du service, `PROBLEM` ou `RECOVERY`, et un message : tout autre second mot laisse Statup tel quel. Une tâche planifiée (cron) peut l'appeler de la même façon.
