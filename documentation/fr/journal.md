---
title: Journal
description: "Ce qui s'est passé sur l'instance, jour par jour, conservé pour les administrateurs et exportable en fichier CSV."
---

Le Journal liste ce qui s'est passé sur l'instance, jour par jour. Les administrateurs l'ouvrent depuis **Paramètres**, **Journal**, puis **Ouvrir le journal**. La sortie du serveur (`docker compose logs statup`) montre les mêmes lignes, en anglais.

## Ce que garde le journal

**Modifier**, en haut, règle ce que garde le Journal et pendant combien de temps.

**Essentiel**, par défaut, garde les gros changements et les problèmes :

- les services ajoutés, modifiés ou supprimés, les paramètres, les canaux d'alerte, les abonnés par courriel retirés et les abonnements des visiteurs ouverts ou fermés ;
- les membres ajoutés, un changement de rôle ou de compte, un événement supprimé ;
- les groupes et les dossiers créés, renommés ou supprimés, les membres d'un groupe, les services d'un dossier, et qui voit un service ou un dossier ;
- une panne détectée et sa fin, un message abandonné vers un canal d'alerte ;
- les connexions bloquées après trop d'essais, et les erreurs de l'application.

**Détaillé** garde tout, à partir du moment où il est choisi : connexions et tentatives refusées, réinitialisations de mot de passe, événements publiés et mis à jour, chaque vérification ratée, chaque message envoyé.

La **Rétention** est de 90 jours par défaut, de 1 à 365. Personne ne peut modifier ni supprimer une ligne : seule la rétention les efface.

## Trouver une ligne

**Rechercher** parcourt les lignes. **Type** les restreint à **Membres**, **Connexions**, **Surveillance**, **Alertes** ou **Système**, **Membre** à une personne, et **Du** et **Au** à une période.

## Exporter

**Exporter** télécharge les lignes affichées, filtres compris, dans un fichier CSV en UTF-8 nommé `journal-YYYY-MM-DD.csv`, de 10 000 lignes au plus. Un tableur l'ouvre directement.

## Ce qui n'est jamais écrit

Les lignes de connexion gardent l'adresse du client, lue comme dans le reste de l'application : voir [`TRUST_PROXY_HEADERS`](configuration.md#trust_proxy_headers). Aucun mot de passe, lien de réinitialisation, adresse de webhook, ni rien de ce qui suit l'hôte d'une adresse vérifiée n'est jamais écrit.
