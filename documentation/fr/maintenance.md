---
title: Maintenances
description: "Annoncer des travaux à l'avance, laisser la maintenance commencer et finir d'elle-même, ou la lancer tout de suite."
---

Une maintenance prévient que des travaux sont prévus ou en cours, sur quels services, et jusqu'à quand. Les éditeurs et les administrateurs en publient une depuis **Nouvel événement**, avec **Type** sur **Maintenance**.

## Annoncer une maintenance

| Champ | Description |
|---|---|
| **Début** | **Maintenant** par défaut. Choisissez un jour et une **Heure de début** pour l'annoncer à l'avance : la liste propose chaque demi-heure, et toute heure peut être saisie. |
| **Durée** | En heures et minutes. La **Fin estimée** suit, comme « Demain à 02:00 ». Sans durée : « Sans fin prévue ». |
| **Services concernés** | Les services touchés par les travaux. |
| **Titre** | Obligatoire. Tant que vous n'en tapez pas, il s'écrit tout seul, comme « Maintenance de Messagerie ». |
| **Description** | Facultative. |

Les heures sont dans le fuseau horaire de l'instance, que le formulaire indique. Sous **Plus d'options**, **Sans interruption** garde les services utilisables pendant les travaux : leur état ne change jamais. Ce choix ne se fait qu'à la création de la maintenance.

**Annoncer la maintenance** la publie et l'envoie aux [canaux d'alerte](alerts.md) qui reçoivent les maintenances.

## À l'avance, ou tout de suite

- **Avec une heure de début**, la maintenance est annoncée : **Programmée**. Ses services ne changent pas avant qu'elle commence.
- **Sans heure de début**, elle commence aussitôt : **En cours**.

## Début et fin automatiques

Chaque minute, Statup démarre chaque maintenance programmée dont l'heure de début est arrivée, et termine chaque maintenance en cours dont l'heure de fin est arrivée. Le début comme la fin sont enregistrés aux heures annoncées, et chacun envoie son alerte.

Une maintenance sans durée ne se termine jamais d'elle-même : terminez-la à la main. Un début ou une fin automatique ne peut pas être annulé.

## À la main

Dans **Publier une mise à jour**, choisissez :

- **En cours** pour la commencer maintenant ;
- **Terminée** pour la terminer maintenant, plus tôt que prévu ;
- **Annulée** pour l'annuler, avant ou pendant les travaux.

## La déplacer

Tant qu'elle est programmée, **Modifier** change son début ou sa durée, et les canaux d'alerte reçoivent le nouveau créneau (« Nouvelle date : … »). Une fois commencée, son début est verrouillé ; sa durée peut encore changer.

## Ce qu'affichent les services

Tant qu'elle est en cours, les services qu'elle cite affichent **En maintenance**, sauf si elle est marquée **Sans interruption**, ou si un état plus grave l'emporte. Une maintenance programmée ne change encore rien.

La [surveillance](monitoring.md#when-no-outage-is-declared) ne déclare aucune panne pour un service en maintenance.

## Sur la page

- Quand rien n'est perturbé, le bandeau indique « Maintenance en cours » pendant les travaux, et « Maintenance prévue le … » quand une maintenance commence dans les sept jours.
- Le bloc **Maintenances** liste celles en cours, puis les programmées par date de début, puis les cinq dernières terminées ou annulées.
- La page de la maintenance indique « dans 2 j » avant qu'elle commence et « commencée il y a 20 min » pendant les travaux, avec le début et la fin prévus sur sa chronologie.

## Après les travaux

Une fois une maintenance terminée, **Publier les nouveautés** ouvre une annonce déjà remplie : « Nouveautés après … », dans la catégorie **Nouveauté**, sur les mêmes services, et faisant suite à cette maintenance. Voir [Annonces](announcements.md).

## Modifier ou supprimer

Les règles sont celles des [incidents](incidents.md#edit-or-delete) : une fois une maintenance **Terminée** ou **Annulée**, seul un administrateur peut encore la modifier ou la supprimer.
