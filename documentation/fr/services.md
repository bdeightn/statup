---
title: Services
description: "Les outils dont on dépend, leur état, comment il change de lui-même, et l'historique de leur disponibilité."
---

Un service est un outil dont on dépend au quotidien : la messagerie, le VPN, l'intranet, la téléphonie, un site web. Chacun affiche sur la page son état et les événements qui le citent.

Les éditeurs et les administrateurs gèrent les services depuis la page **Services**. Les lecteurs et les visiteurs les voient sur le tableau de bord, où un service s'ouvre dans un panneau latéral avec son état, ses 30 derniers jours et ses derniers événements.

## Créer un service

**Nouveau service**, sur la page Services, ouvre le formulaire :

| Champ | Description |
|---|---|
| **Nom** | Obligatoire, 100 caractères au plus. Deux services peuvent porter le même nom. |
| **Description** | Facultative, une phrase sur ce à quoi sert le service, 500 caractères au plus. |
| **Surveillance** | **Aucune**, **Web** ou **Port** : voir [Surveillance](monitoring.md). |
| **Icône** | Facultative : une icône fournie, ou une icône que vous envoyez. |

**Créer le service** l'ajoute en fin de liste, dans l'état **Opérationnel**. Les administrateurs peuvent aussi ajouter des services lors du [premier lancement](install.md#first-launch).

Pour modifier un service par la suite, ouvrez-le par son nom ou par son crayon sur la page Services, puis validez avec **Enregistrer**. Son état ne figure pas dans le formulaire : il se règle depuis la liste.

## Icônes

- 24 icônes fournies, de Serveur à Impression.
- **Envoyer une icône** : PNG, JPG, WebP ou SVG, jusqu'à 256 Ko. Les images de plus de 128 pixels sont réduites ; un SVG est débarrassé de tout ce qui pourrait exécuter ou charger quelque chose.
- Les icônes envoyées forment une seule bibliothèque, partagée par les services, les événements et les modèles, sous **Icônes** sur la page Services. Une icône encore utilisée ne peut pas être supprimée.

## États

| État | À la main | Automatiquement |
|---|---|---|
| **Opérationnel** | Oui | |
| **Dégradé** | Oui | Par un incident ouvert d'impact **Mineur**, ou sans impact |
| **En panne** | Oui | Par un incident ouvert d'impact **Majeur**, ou par la [surveillance](monitoring.md) |
| **En maintenance** | Oui | Par une maintenance en cours, sauf si elle est marquée **Sans coupure** |

### Régler un état à la main

Sur la page Services, chaque service a un menu d'état. Un choix est publié aussitôt, avec **Revenir à l'état précédent** à côté. **En panne** demande d'abord « Afficher « En panne » à tous les visiteurs ? », puis **Publier**.

### Quand un état change de lui-même

Un incident fixe l'état des services qu'il cite tant qu'il est **En analyse** ou **En correction**. Une maintenance le fait tant qu'elle est **En cours**. Une annonce, jamais. Voir [Incidents](incidents.md) et [Maintenances](maintenance.md).

Quand plusieurs causes s'appliquent, le service affiche la plus grave : **En panne**, puis **Dégradé**, puis **En maintenance**, puis **Opérationnel**. Une maintenance en cours ne masque pas une panne détectée, et un état réglé à la main sur **En panne** le reste pendant un incident mineur.

Quand l'incident passe **Sous surveillance**, est résolu ou annulé, ou quand la maintenance se termine, le service revient à la plus grave des causes restantes, en général l'état réglé à la main.

Tant qu'un événement ou une panne détectée fixe l'état d'un service, son menu laisse la place à l'état affiché, avec un lien vers cet événement, ou à **Détecté automatiquement** et **Déclarer un incident**. Le menu revient quand l'un ou l'autre prend fin.

## Ordre

**Changer l'ordre** place une flèche vers le haut et une vers le bas sur chaque service. L'ordre est enregistré peu après le dernier déplacement, et s'applique partout où les services sont listés, tableau de bord compris. **Terminer** ou Échap quitte ce mode.

Dans ce même mode, **Mettre en tête les services qui ont un problème** place chaque service qui n'est pas **Opérationnel** avant les autres, chaque groupe dans l'ordre choisi. Cette option est désactivée par défaut.

## Historique de disponibilité

Chaque service a une barre par jour, dans le fuseau horaire de l'instance, jusqu'à aujourd'hui. Le tableau de bord et la page Services montrent 90 jours quand la place le permet, et les 30 derniers sur un écran étroit ; le panneau latéral en montre toujours 30.

| Barre | Quand |
|---|---|
| Verte | Aucun incident ce jour-là |
| Orange | Un incident **Mineur** ou sans impact, ou une panne détectée de quinze minutes ou plus |
| Rouge | Un incident **Majeur** |
| Grise | Avant que le service existe |

- L'incident le plus grave de la journée l'emporte. Un incident compte de son début jusqu'à son passage **Sous surveillance**, ou jusqu'à sa fin ; un incident annulé ne compte jamais.
- Une maintenance ne colore jamais une journée.
- Une panne détectée ne colore une journée que si aucun incident ne la couvre, et seulement si elle dure quinze minutes ou plus au total.

Aucun pourcentage : la légende compte les jours avec incident.

## Supprimer un service

Un service qu'aucun événement n'a jamais cité peut être supprimé depuis son formulaire, avec **Supprimer le service**. Il disparaît sans laisser de trace, pannes détectées comprises.

Un service cité par des événements est conservé, pour que leur historique reste complet. Pour ne plus le montrer sur le tableau de bord, un administrateur le décoche dans **Organiser** : voir [Paramètres](settings.md#arrange-the-dashboard). Un service décoché revient dès qu'il n'est plus **Opérationnel**.

## Qui peut faire quoi

| | Lecteur | Éditeur | Administrateur |
|---|---|---|---|
| Voir les états et l'historique, ouvrir le panneau latéral d'un service | Oui | Oui | Oui |
| Créer, modifier, régler un état, changer l'ordre, supprimer, gérer les icônes | | Oui | Oui |
| Retirer un service du tableau de bord | | | Oui |
