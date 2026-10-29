# AP Football Manager — validation du mercato et du jour de clôture (mission 10)

29/10/2026. Complète `docs/APFM_TRANSFER_REALISM_REPORT.md`. Mesures faites avec le harnais `apfm_economy` (production : mêmes points d'entrée jour par jour et bascule de saison que le jeu), monde FM24 révision 5, binaires « avant » (HEAD de départ + compteurs de harnais seulement) et « après » (cette mission). Dépouillement : `tools/m10_compare.py`.

## 1. Protocole

| | Échantillon rapide | Validation longue |
|---|---|---|
| Graines | 11 et 7 | 21 |
| Clubs de l'utilisateur (passif) | `fm-c-498`, `fm-c-1113` | `fm-c-498` |
| Saisons | 3 | 5 |
| Périmètre | par défaut (18 clubs actifs, les autres achètent un jour sur sept) | monde entier actif (`--everything`) |
| Simulations | 4 avant + 4 après | 1 avant + 1 après |

Les 239 clubs IA des cinq pays (Angleterre 68, France 53, Espagne 42, Italie 40, Allemagne 36) sont couverts dans chaque simulation, quel que soit le club de l'utilisateur ; les deux clubs choisis ne changent que la partie contrôlée. **Les quatre simulations par version donnent des totaux quasi identiques** (305 ± 4 transferts par saison, 49–50 % d'international) : l'échantillon est étroit, il montre un comportement stable, pas une dispersion large.

Une phase est celle de la **journée où le transfert a eu lieu** (fenêtre de 60 jours : 30 avant le début de saison, 30 après). Une première version du comptage utilisait l'état de la fenêtre à la fin du jour, donc décalé d'un jour ; corrigé avant les chiffres ci-dessous (il n'y a plus de transfert « hors fenêtre »).

## 2. Activité par phase (échantillon rapide, par saison)

| Phase | Jours | Transferts avant | Transferts après | Par jour avant → après | Prêts avant → après |
|---|---|---|---|---|---|
| Début (> 40 jours restants) | 20 | 111,4 | 41,5 | 5,6 → 2,1 | 53,6 → 20,1 |
| Milieu (8 à 40) | 33 | 128,9 | 184,6 | 3,9 → 5,6 | 55,4 → 96,7 |
| Dernière semaine (3 à 7) | 5 | 11,0 | 27,7 | 2,2 → 5,5 | 6,8 → 19,7 |
| 72 dernières heures (1 à 2) | 2 | 5,1 | 39,5 | 2,6 → 19,8 | 3,2 → 54,0 |
| Dernier jour | 1 | 2,4 | 11,6 | 2,4 → 11,6 | 1,2 → 33,0 |
| **Total** | 61 | **258,8** | **304,8** | | **120,2 → 223,4** |

Avant, le rythme **baissait** vers la fermeture (5,6 par jour au début, 2,4 le dernier jour) : les besoins étaient réglés dès que possible. Après, il monte de 2,1 par jour (début) à 19,8 (72 h) : les clubs sans besoin pressant attendent au début, les acheteurs tiennent à un prix plus bas que les vendeurs tant que le temps est devant eux, puis l'urgence rapproche les deux. Le total ne monte que de 18 %.

Ce n'est pas une courbe réaliste prouvée : c'est une courbe **cohérente** avec ses causes. Le dernier jour (11,6) est moins actif que les 72 heures par jour (19,8) parce que beaucoup d'affaires sont déjà faites la veille.

## 3. Prix

Prix payé / valeur du joueur (médiane, échantillon rapide) :

| | Avant | Après |
|---|---|---|
| Tous | 0,90 | 0,78 |
| National / international | 0,90 / 0,90 | 0,78 / 0,80 |
| Début / milieu | 0,90 / 0,90 | 0,77 / 0,82 |
| Dernière semaine | 0,90 | 0,77 |
| 72 dernières heures | 0,90 | 0,71 |
| Dernier jour | 0,90 | 0,68 |
| Prix médian payé (£) | — | 273 874 (s. 1) → 463 316 (s. 3) |

Avant, le prix était un multiplicateur presque plat (0,7 à 1,05). Les ventes sont maintenant des surplus et des contrats courts, vendus moins cher à mesure que le temps manque (cas C, D et E du cahier des charges). **Le prix médian baisse** : c'est un effet de la composition des ventes plus qu'une décote générale, mais je n'ai pas de référence externe pour dire que 0,78 est « juste ».

## 4. Géographie

Part internationale : 22,8 % → 49,7 % (échantillon rapide), 20,4 % → 52,2 % (validation longue). Détail dans le rapport (par tranche de réputation de l'acheteur : 42,5 % / 46,0 % / 67,7 %, et par pays vendeur : 31 % à 71 %). **Elle n'est pas un objectif, et elle est probablement trop haute** (§ 8 du rapport). Elle est stable d'une phase à l'autre (48–58 %) : le comportement ne change pas de nature à la clôture.

## 5. Budgets, effectifs, intégrité (échantillon rapide, fin de saison 3)

| | Avant | Après |
|---|---|---|
| Clubs à trésorerie négative (%) | 3,8 / 3,4 / 4,2 / 3,8 | 3,4 / 4,6 / 2,5 / 3,4 |
| Clubs à plus de 150 % de la masse salariale | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 |
| Effectif sénior minimum (médiane) | 18–19 (27) | 18–19 (27) |
| Joueurs en double, prêts incohérents | 0 | 0 |

Rien de mesurable n'a bougé. Les seuils de la mission 8 (0,2 % de trésorerie négative à la saison 10) **ne sont pas atteints dans ce protocole même avant mes changements** (3–4 % à la saison 3) : ce protocole n'est pas celui de la mission 8, je n'ai pas tenté de le reproduire.

## 6. Validation longue (1 graine, 5 saisons, monde entier actif)

Par saison — transferts IA / prêts :

| Saison | Avant | Après |
|---|---|---|
| 1 | 686 / 915 | 465 / 791 |
| 2 | 289 / 155 | 368 / 608 |
| 3 | 221 / 54 | 388 / 482 |
| 4 | 204 / 45 | 355 / 406 |
| 5 | 323 / 100 | 405 / 459 |

Le premier mercato avant était gonflé : le monde de départ lâche d'un coup ses transferts dans les 20 premiers jours (1 125 des 1 723 transferts de la période, toutes saisons confondues, sont dans la phase « début »). Après, 945 (patience de 65 % des clubs). Les saisons suivantes, avant, descendent à ~200–320 ; après, elles restent à 355–405, **au-dessus ou à la limite de la fourchette 200–365 de la mission 8** (368, 388, 405 aux saisons 2, 3 et 5) : le marché est plus actif qu'avant, pas calibré sur une cible. Les prêts retombaient à ~50 par saison à partir de la saison 3 (le stock initial s'épuisait) ; ils se maintiennent maintenant (400–600), parce que les listes sont alimentées chaque semaine.

Activité par phase sur 5 saisons (transferts / prêts) : début 945 / 499, milieu 883 / 1 398, dernière semaine 93 / 509, 72 h 28 / 247, dernier jour 32 / 93 (avant : 1 125 / 585, 515 / 558, 54 / 79, 20 / 32, 9 / 15). Dans ce mode, **les transferts de fin de fenêtre ne montent que modestement** (28 sur 5 saisons à 72 h, contre 20) mais les prêts, eux, montent beaucoup. Le mode « monde entier » fait acheter tous les clubs tous les jours : leurs besoins sont réglés bien avant la fin.

État après 5 saisons (31 juin 2031) :

| | Avant | Après |
|---|---|---|
| Clubs à trésorerie négative | 4,6 % | 5,0 % |
| Clubs à plus de 100 / 110 / 150 % de la masse salariale | 123 / 37 / 5 | 122 / 42 / **7** |
| Effectif sénior (p10 / médiane / p90) | 19 / 23 / 27 | 20 / 23 / 26 |
| Intégrité (joueurs en double, prêts incohérents) | 0 | 0 |

Deux clubs de plus à plus de 150 % de la masse salariale : trop peu pour conclure (bruit d'un monde de 239 clubs), mais **je ne peux pas non plus conclure à l'absence d'effet**. À surveiller dans la mission performance (simulation longue sur 10 saisons non refaite).

## 7. Performance

| Configuration | Avant | Après | Écart |
|---|---|---|---|
| 3 saisons, périmètre par défaut, 4 simulations en parallèle : total | 74,1 s | 78,3 s | +5,7 % |
| Idem : médiane d'un jour | 63,5 ms | 64,7 ms | +1,9 % |
| 5 saisons, monde entier actif, 2 en parallèle : total | 91,6 s | 92,8 s | +1,3 % |
| Évaluations d'acheteurs (monde entier, 5 saisons) | 72 895 | 57 513 | −21 % |

Une première version coûtait +20 % : la mise en liste de prêt par statut tournait toute l'année alors qu'un prêt ne peut commencer que fenêtre ouverte. Corrigé (`list_loans_by_status` ne fait rien fenêtre fermée) et couvert par un test. Les évaluations d'acheteurs baissent parce que la patience du début de fenêtre fait attendre des clubs.

**Ralentissement de la saison 3** : non reproduit. Dans le harnais, les saisons durent 19–22 s (monde entier) ou 24–33 s (périmètre par défaut) sans dérive de la saison 1 à 5. Celui de la mission 8 a été mesuré dans le vrai binaire (×8) : non testé ici, **cause ouverte**, à traiter dans la mission performance suivante. Je n'ai pas cherché à le résoudre.

## 8. Ce que ces mesures ne montrent pas

- Que 49,7 % d'international, 0,78 de prix/valeur ou 223 prêts par saison soient les bonnes valeurs. Ce sont les sorties d'un système cohérent dont chaque règle est expliquée et bornée.
- Les offres reçues par les joueurs de l'utilisateur : le harnais a un manager passif (aucun joueur du club n'est mis en vente). Elles sont couvertes par les tests de scénario (C) et par les 130 tests de transferts existants, qui passent, mais pas par une carrière jouée.
- Une carrière de dix saisons : mission demandée à 5 saisons, une graine.
