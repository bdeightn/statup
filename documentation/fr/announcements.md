---
title: Annonces
description: "Informer de ce qui est nouveau ou de ce qu'il faut savoir, sans toucher à l'état d'aucun service."
---

Une annonce est une information : la nouvelle version d'un outil, une imprimante qui a changé de place, un client à installer sur tous les postes. Les éditeurs et les administrateurs en publient une depuis **Nouvel événement**, avec **Type** sur **Annonce**.

## Publier une annonce

| Champ | Description |
|---|---|
| **Catégorie** | **Information**, par défaut, ou **Nouveauté**. |
| **Services concernés** | Facultatif. Ils s'affichent comme étiquettes ; leur état ne change jamais. |
| **Titre** | Obligatoire, 200 caractères au plus. |
| **Description** | Facultative. |

Sous **Plus d'options**, **Fait suite à une maintenance** la rattache à l'une des vingt dernières maintenances terminées ou annulées, et **Enregistrer comme modèle** la garde comme [modèle](incidents.md#templates).

**Publier l'annonce** l'affiche aussitôt et l'envoie aux [canaux d'alerte](alerts.md) qui reçoivent les annonces.

## Après une maintenance

Sur une maintenance terminée, **Publier les nouveautés** ouvre une annonce déjà remplie : le titre « Nouveautés après … », la catégorie **Nouveauté**, les mêmes services, et la maintenance à laquelle elle fait suite.

## Mises à jour

Une annonce n'a pas d'avancement et ne se clôt jamais. On peut tout de même lui ajouter des mises à jour, sous forme de messages, et les éditeurs peuvent toujours la modifier.

## Durée d'affichage

Une annonce n'expire jamais. Elle reste dans la liste des événements, dans le bloc **Activité récente** et dans le flux jusqu'à ce que des événements plus récents prennent sa place : le bloc montre les dix derniers, le flux les cinquante derniers. Elle n'apparaît jamais dans le bandeau d'état, et ne compte pas dans le nombre de nouveaux événements depuis la dernière visite d'un membre.
