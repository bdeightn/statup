---
title: La page d'état
description: "Ce que lisent les collègues et les visiteurs : le bandeau, les services, l'activité récente, les événements et le flux."
---

Ce que tout le monde lit, depuis un poste ou un téléphone. Avec **Qui peut voir la page** sur **Tout le monde**, les visiteurs la lisent sans compte ; avec **Membres seulement**, ils doivent se connecter. Les noms des membres de l'équipe ne sont montrés qu'aux membres connectés.

## Le tableau de bord

La page s'actualise d'elle-même chaque minute tant qu'elle est ouverte, et quand le lecteur revient sur son onglet. Si la connexion tombe, elle indique « Connexion perdue, nouvel essai en cours. »

Les administrateurs choisissent ses blocs et leur disposition : voir [Paramètres](settings.md#arrange-the-dashboard). Les membres connectés trouvent un lien **Documentation** au pied de chaque page, vers ces pages.

### Le bandeau d'état

Toujours en haut, il donne d'abord la réponse :

- « Tous les services fonctionnent normalement » ;
- ou « 2 services perturbés », avec une ligne pour chacun, le plus grave en premier : son état, l'événement qui l'explique et depuis quand, la prochaine mise à jour annoncée, et le début de la dernière mise à jour ;
- ou « Maintenance en cours », quand des travaux sont en cours et que rien n'est perturbé.

Une panne trouvée par la [surveillance](monitoring.md) et pas encore déclarée s'affiche « Détecté automatiquement », avec sa durée. Quand rien n'est perturbé, le bandeau annonce aussi une maintenance prévue dans les sept jours.

### Services

Chaque service avec son état et sa disponibilité jour par jour. Un service s'ouvre dans un panneau latéral : son état, ses 30 derniers jours, ses trois derniers événements. Voir [Services](services.md).

### Activité récente

Les dix derniers événements, par jour. Par défaut, le bloc montre les incidents et les annonces, pas les maintenances. **Être prévenu** et **Tout voir** mènent plus loin.

### Maintenances

Les maintenances en cours, puis celles qui sont prévues, puis les cinq dernières terminées ou annulées. Voir [Maintenances](maintenance.md).

## Événements

La page **Événements** liste tous les événements, du plus récent au plus ancien, 20 par page.

- **Rechercher** cherche des mots dans les titres et les descriptions, pas dans les mises à jour. Chaque mot peut être le début d'un mot plus long ; ni la casse ni les accents ne comptent.
- **Filtres** : **Type**, **Avancement** (**En cours ou à venir**, ou **Terminés**), **Service**, et **Du** et **Au**, selon le jour de création de l'événement.

Un événement s'ouvre dans un panneau latéral sans quitter la liste ; sur un téléphone, il ouvre sa page. Sa page contient ses **Détails** et sa **Chronologie**, du plus récent au plus ancien ; une annonce montre son texte, puis ses **Mises à jour**.

Les membres connectés voient sur **Événements** combien d'incidents et de maintenances ont été publiés depuis leur dernière visite.

## Être prévenu

**Être prévenu**, en bas de chaque page et sur le bloc **Activité récente**, propose trois façons de suivre sans ouvrir la page :

- **Avec un lecteur de flux** : l'adresse du flux Atom, à copier.
- **Dans Teams, Slack ou par courriel** : par les canaux qu'un administrateur règle dans [Alertes](alerts.md).
- **Ou cette page** : laissée ouverte, elle se met à jour toute seule chaque minute.

Un visiteur ne peut pas encore s'abonner avec sa propre adresse e-mail.

## Le flux Atom

`/feed` contient les cinquante derniers événements : leur type, leur impact et leur avancement, leurs services, leur description, leur bilan et leurs mises à jour, sans le nom d'aucun membre. Avec **Membres seulement**, un lecteur de flux sans session est renvoyé vers la connexion : il ne peut donc plus lire le flux. Ses liens utilisent [`PUBLIC_URL`](configuration.md#public_url) quand ce réglage est défini.

## Langue et thème

Statup parle français, anglais, allemand et espagnol. Il utilise, dans l'ordre : la langue choisie avec le sélecteur de langue, conservée sur le compte d'un membre connecté ; puis la langue du navigateur ; puis [`DEFAULT_LOCALE`](configuration.md#default_locale).

Le thème clair ou sombre suit celui du système tant que le lecteur n'en a pas choisi un ; chaque navigateur garde son propre choix.
