---
title: Introduction
description: "Ce qu'est Statup, et les trois éléments qui le composent : les services, les événements et les personnes."
---

Statup est une page d'état que vous hébergez vous-même. Celui qui gère un service publie, une seule fois, ce qui est en panne, ce qui est prévu et ce qui a changé. Tous les autres le lisent en clair, depuis leur poste ou leur téléphone, sans compte quand la page est ouverte à tout le monde.

Les outils de supervision sont faits pour ceux qui réparent. Statup est fait pour ceux qui s'en servent : il répond à « C'est en panne ? » avant même qu'on pose la question.

## Les services

Les outils dont on dépend au quotidien : la messagerie, le VPN, l'intranet, la téléphonie. Chacun affiche son état, **Opérationnel**, **Dégradé**, **En panne** ou **En maintenance**, ainsi que sa disponibilité jour par jour. Statup peut aussi vérifier de lui-même qu'un service répond. Voir [Services](services.md) et [Surveillance](monitoring.md).

## Les événements

Ce qui se publie :

- un **incident** quand quelque chose tombe en panne, avec ses mises à jour jusqu'à sa résolution : voir [Incidents](incidents.md) ;
- une **maintenance** quand des travaux sont prévus, avec un début et une fin qui s'appliquent d'eux-mêmes aux heures annoncées : voir [Maintenances](maintenance.md) ;
- une **annonce** pour une nouveauté ou une information : voir [Annonces](announcements.md).

Un incident fixe l'état des services qu'il cite tant qu'il est en analyse ou en correction, une maintenance tant qu'elle est en cours. Chaque événement peut aussi être envoyé dans Teams, Slack, par courriel ou vers d'autres outils : voir [Alertes](alerts.md).

## Les personnes

- Les **lecteurs** lisent la page et les événements. Quand la page est **Publique**, les visiteurs la lisent sans compte.
- Les **éditeurs** publient les événements et fixent l'état des services.
- Les **administrateurs** gèrent en plus les paramètres, l'équipe, les alertes et l'organisation de la page.

Voir [Équipe et rôles](team.md).

## Ce qu'il faut pour le faire tourner

Un binaire et un fichier SQLite, dans une image Docker d'environ 11 Mo, pour serveurs x86-64 et ARM. Aucune autre base de données, aucun cache à faire tourner.

Les polices et les scripts sont servis par votre instance. Le serveur ne contacte que les adresses que vous lui demandez de vérifier ou d'alerter, et GitHub une fois par jour pour chercher une nouvelle version, ce que [`UPDATE_CHECK=false`](configuration.md#update_check) désactive.

Statup est gratuit et open source, sous licence AGPL-3.0. Commencez par l'[installation](install.md).
