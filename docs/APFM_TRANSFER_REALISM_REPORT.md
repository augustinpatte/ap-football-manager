# AP Football Manager — mercato réaliste (mission 10)

29/10/2026. Fork `~/apfm-migration/openfootmanager`, branche `apfm/main`, **commit local, rien de poussé**. `world.db`, moteur de match 2D, staff et coupes non touchés. Monde FM24 inchangé (révision 5).
Outils : `~/apfm-migration/tools/m10_compare.py` (comparaison avant/après), harnais `apfm_economy` (compteurs ajoutés : phase de la fenêtre, frontière, prix/valeur, tranche de réputation de l'acheteur). Validation détaillée : `docs/APFM_DEADLINE_DAY_VALIDATION.md`.

## 1. Verdict

Le marché existant a été **corrigé, pas remplacé** : même balayage quotidien, mêmes prêts, mêmes contrats, mêmes budgets. Ce qui change, c'est la façon dont un club **choisit**, **fixe un prix** et **réagit à la fermeture**.

| Mesure (4 simulations de 3 saisons, 2 graines × 2 clubs) | Avant | Après |
|---|---|---|
| Transferts IA par saison | 259 | 305 |
| Part de transferts internationaux | 22,8 % | 49,7 % |
| Prêts par saison | 120 | 223 |
| Transferts par jour, 20 premiers jours | 5,6 | 2,1 |
| Transferts par jour, milieu | 3,9 | 5,6 |
| Transferts par jour, dernière semaine (3 à 7 jours) | 2,2 | 5,5 |
| Transferts par jour, J-2 et J-1 | 2,6 | 19,8 |
| Transferts le jour de fermeture | 2,4 | 11,6 |
| Prix / valeur (médiane) | 0,90 partout | 0,77 début, 0,82 milieu, 0,71 J-2/J-1, 0,68 dernier jour |
| Doublons, joueurs en double, prêts incohérents | 0 | 0 |
| Temps de simulation (même configuration, 4 en parallèle) | 74,1 s | 78,3 s (+5,7 %) |

Le nombre de transferts n'est **pas** la preuve du réalisme (il monte de 18 %, dans la fourchette de la mission 8 : 200 à 365). La preuve, ce sont les décisions : voir les scénarios (§5) et les limites (§8).

La règle des « 90 % » : il n'y avait pas de quota chiffré. C'était une **recherche à deux temps** dans `evaluate_transfer_market` : un club regardait d'abord le marché de son pays et ne passait à l'étranger que si rien n'y convenait. Mesuré avant : 77 % de transferts nationaux (pas 90 %), 10 % pour les vendeurs anglais. Elle est supprimée.

## 2. Fonctions modifiées

| Fichier | Changement |
|---|---|
| `transfers/market.rs` `evaluate_transfer_market` | Fin de la recherche nationale d'abord. Phase de la fenêtre lue une fois. Statuts calculés une fois par balayage. Prix demandé calculé par joueur (statut, contrat, potentiel, moral, santé financière du vendeur, temps restant). Chaque acheteur pèse jusqu'à 6 options dans son pays et 6 à l'étranger, et prend la meilleure. Prix maximal acheteur, refus d'un club surendetté, refus d'un achat cher qui n'améliore pas l'équipe. Plafond quotidien de transferts et de prêts multiplié par le facteur d'activité. Clubs avec un poste vacant : acheteurs tous les jours dans la dernière semaine. Tous les clubs : acheteurs tous les jours dans les 3 derniers jours. Les clubs sans besoin urgent attendent au début de la fenêtre. |
| `transfers/market.rs` `squad_levels_by_club` | Un joueur blessé 45 jours ou plus ne compte plus : le club a un besoin. |
| `transfers/market.rs` `minimum_acceptable_fee` → `minimum_acceptable_fee_at` | Prix plancher d'une offre du joueur : un titulaire coûte plus cher près de la fermeture, un remplaçant moins, un contrat qui finit dans 60 jours moins. Appelé par `make_transfer_bid`. Une seule valeur ajoutée au multiplicateur puis bornée : rien ne s'accumule entre les tours. |
| `transfers/lifecycle.rs` | `clubs_in_cooldown_of` / `within_cooldown_of` : délai avant qu'un club refusé revienne, raccourci avec la fermeture (30 j, 15, 7, 3). Le délai de 30 jours reste la règle partout ailleurs. |
| `transfers/loans.rs` `list_loans_by_status` (nouveau) | Mise en liste de prêt par statut, fenêtre ouverte seulement. |
| `ai_squad_sales.rs` | Appelle `list_loans_by_status` à la revue hebdomadaire du club. |
| `transfers/mod.rs`, `consts.rs` | `MarketTarget` porte valeur et statut ; `MarketCandidate` perd deux champs devenus inutiles ; `OPTIONS_WEIGHED_PER_BUYER`. |
| `transfers/realism.rs` (nouveau, 811 lignes dont 340 de tests) | Toutes les fonctions pures ci-dessous. |

## 3. Règles supprimées

- Recherche « pays du club d'abord, étranger seulement si rien ne convient ».
- Prix unique `suggested_incoming_fee` (0,7 à 1,05 × valeur) pour les joueurs des clubs IA (il reste pour les offres reçues par les joueurs de l'utilisateur : son prix est le sien à fixer).
- Seuil de mise en vente `listé ou score > 60` pour tous les joueurs, quel que soit leur rôle.
- Un tour d'acheteur gaspillé : avant, un acheteur choisissait une cible puis découvrait que le club ne la vendait pas, et perdait sa journée.

## 4. Règles ajoutées (toutes calculées, bornées, sans état enregistré)

**Phases de la fenêtre** (jours restants) : début (> 40), milieu (8 à 40), dernière semaine (3 à 7), dernières 72 h (1 à 2), dernier jour (0). Une pression de 0 à 1 (0 / 0 / 0,35 / 0,65 / 1) alimente tous les termes dépendant du temps. Sans date de fenêtre connue, c'est le milieu : le comportement d'avant.

**Statut du joueur** (indispensable, titulaire, rotation, remplaçant, surplus, indésirable), calculé le jour même : rang dans son poste, onze de départ, écart avec la première doublure, apparitions, âge, part du salaire dans le budget salarial, club en difficulté, potentiel, blessure longue. Un indésirable est un surplus **coûteux** (≥ 4 % du budget salarial hebdomadaire ou club en difficulté) de 24 ans ou plus : un jeune cher n'est pas un indésirable.

**Trois prix distincts** (`realism.rs`) :
1. valeur : `market_value`, inchangée ;
2. **prix demandé** = valeur × (1 + termes additifs), borné à 0,50–1,60 : liste −0,10 ; statut de +0,25 (indispensable) à −0,18 (indésirable) ; contrat ≤ 60 j −0,30, ≤ 180 j −0,18, ≤ 365 j −0,06, ≥ 3 ans +0,06 pour un titulaire ; potentiel d'un jeune jusqu'à +0,10 ; moral ≤ 40 −0,04 ; vendeur en difficulté −0,06 ; effet du temps : indispensable difficile à remplacer +0,20 × pression, rotation 0, remplaçant −0,04, surplus −0,18, indésirable −0,30, contrat ≤ 60 j −0,10 ;
3. **prix maximal acheteur** = valeur × (0,95 + 0,02 × amélioration sur la première doublure, jusqu'à 12 ; + 0,10 à 0,20 si le poste est dégarni ; − pénalité d'âge ≥ 30 ans jusqu'à −0,20 ; − « patience » de 0,15 au début, 0,08 au milieu, 0,03 la dernière semaine, rien en fin de fenêtre, et +0,03 le dernier jour), borné à 0,60–1,50 et **jamais au-delà de ce que le club a** (budget transferts et trésorerie).

L'accord se fait au prix demandé s'il est sous le maximum, au maximum si celui-ci est à moins de 8 % du prix demandé, sinon il n'y a pas d'affaire. Aucune valeur négative, infinie ou cumulée n'est possible (testé aux bornes, y compris valeur 0 et valeur énorme).

**Géographie.** Un poids multiplicatif sur le classement des options d'un acheteur, jamais une exclusion et jamais un quota : 1,0 pour un joueur du pays ; à l'étranger, de 0,30 (petit club) à 0,75 (grand club) selon la réputation de l'acheteur, plus jusqu'à 0,30 si le joueur est une vraie amélioration, moins 0,15 si le prix absorbe plus de 60 % du budget d'un petit club. Classement = (intérêt + 4 × amélioration) × poids.

**Fin de fenêtre** : facteur d'activité du plafond de transferts et de prêts (1,0 jusqu'au milieu, 1,42, 1,78, 2,2) ; délai de relance raccourci ; surplus et indésirables mis en vente de fait à partir du milieu, avec un bonus d'attrait croissant ; remplaçants vendables dans la dernière semaine.

**Prêts.** Jusqu'à 4 joueurs en liste de prêt par club (2 à l'ouverture auparavant) : jeunes de 22 ans ou moins avec au moins +6 de potentiel qui ne jouent pas ; surplus et indésirables dès la dernière semaine, même déjà en vente (le prêt est la sortie quand la vente échoue) ; indésirables coûteux dès le milieu. Jamais un joueur du onze, ni un contrat de moins de 90 jours. Les clauses de prêt (partage salarial, option d'achat, retour) sont **celles du système existant**.

**Besoins et budgets.** Un club à plus de 120 % de sa masse salariale ou à trésorerie négative n'achète pas avec frais (il vend ou prête). Un achat qui dépasse 15 % du budget transferts doit améliorer la première doublure d'au moins 3 points, ou combler un poste dégarni.

## 5. Scénarios testés (déterministes, `tests/transfers_tests/realism.rs` + `ai_squad_sales.rs`)

| | Scénario | Résultat |
|---|---|---|
| A | Grand club anglais (900), joueur de 80 en France, option nationale de 66 | recrute en France |
| B | Petit club français (450), option nationale 68 / étrangère 70 | reste national ; **va à l'étranger** si le national est à 61 et l'étranger à 88 |
| C | Titulaire indispensable, même offre à J-20 et à J-2 | acceptée à J-20, contre-offre plus haute à J-2 |
| D | Surplus à J-20 et à J-2 | vendu moins cher à J-2 (écart ≥ 400 000 £ sur 4 M) |
| E | Fin de contrat dans 24 jours, J-2 | prix ≤ 80 % de celui d'un contrat de 2 ans |
| F | Club riche, personne à améliorer, dernier jour | aucun achat, journal de transferts vide |
| G | Club sans gardien, dernier jour | recrute un gardien |
| H | Club à 100× son budget salarial | refuse un joueur de 85 à 2 M |
| I | Jeune joueur en liste de prêt | emprunté, partage salarial 50–100 %, **sauvegarde/rechargement identique**, retour au club propriétaire après la date ; propriétaire d'un titulaire refuse un prêt à 20 % |
| J | Transfert international puis sauvegarde/rechargement | propriétaire et trésoreries identiques (frais payés = frais reçus) |
| K | Fenêtre fermée | aucune transaction IA ; liste de prêt non alimentée |
| L | Deux acheteurs, un joueur | un seul transfert, un seul propriétaire, journal à une ligne |

Plus 15 tests unitaires de fonctions sur les bornes (phases, pression, prix demandé pour tous les statuts × phases × contrats × âges incluant valeur 0 et valeur ≈ 4×10¹², prix maximal, accord, géographie avec budget nul, patience, besoins).

## 6. Transferts nationaux / internationaux

Moyenne de 4 simulations de 3 saisons (graines 11 et 7 × deux clubs).

| Acheteur (réputation) | Transferts | Part internationale |
|---|---|---|
| < 600 | 1 682 | 42,5 % |
| 600–749 | 1 076 | 46,0 % |
| ≥ 750 | 900 | 67,7 % |

Les grands clubs sortent plus des frontières, les petits moins : le gradient est voulu. Part internationale selon le pays vendeur : France 31 % (marché le plus autosuffisant, 54 clubs), Angleterre 43 %, Espagne 55 %, Allemagne 60 %, Italie 71 % (peu de clubs, beaucoup de surplus). Avant : 10 % à 34 %.

**Cette part n'est pas un objectif** et je la juge **probablement trop haute** : aucune référence fiable dans ce monde à cinq pays (les vrais clubs achètent aussi hors de ces cinq pays, ce qui rend les transferts internes plus rares qu'ici). Une décision de réglage pourrait la ramener vers 35–40 % ; je n'ai pas voulu l'ajuster pour tomber sur un chiffre.

## 7. Prix et prêts

- Prix/valeur médian : 0,78 (avant : 0,90 constant). Baisse de la fourchette de prix des joueurs en vente : ils sont désormais majoritairement des surplus, des contrats courts et des mises en vente. La part de clubs à trésorerie négative ne bouge pas (voir `APFM_DEADLINE_DAY_VALIDATION.md`).
- Prix/valeur selon la phase : début 0,77 ; milieu 0,82 ; dernière semaine 0,77 ; J-2/J-1 0,71 ; dernier jour 0,68. Les surplus baissent, la fin de la fenêtre n'est pas un surcoût général (les indispensables dont le remplacement est difficile montent, mais ils sont rarement en vente).
- Prêts : 223 par saison (avant : 120). Répartition par phase : début 20, milieu 97, dernière semaine 20, J-2/J-1 54, dernier jour 33 (avant : 54, 55, 7, 3, 1).

## 8. Limites restantes

1. **Part internationale ≈ 50 % : probablement trop haute**, voir § 6. Les petits clubs sont à 42 %.
2. Le prix de réserve de l'acheteur (« patience ») concentre les transferts vers la fin de la fenêtre dans le mode « périmètre actif » (J-2/J-1 : ×8 par jour). Dans le mode « monde entier actif » (tous les clubs jouent chaque jour), la fin est moins marquée (dernier jour 9 → 32 sur 5 saisons).
3. Les joueurs indispensables ne sont presque jamais vendus (ils ne sont mis en vente que sur décision du club, et le club n'en met pas) : l'effet « prix plus élevé » est surtout visible dans les offres de l'utilisateur.
4. Promotion et relégation ne sont pas des signaux explicites : le club promu ou relégué réagit via son budget salarial et sa trésorerie, déjà recalculés par la revue annuelle. Il n'y a pas de règle propre « promu = renforce plusieurs postes ».
5. Aucun terme « salaire futur » ni « alternatives disponibles » dans le prix maximal : la limite salariale est celle déjà en vigueur (`buyer_can_pay_standard_wage` et `SigningBooks::can_carry`), et le champ `alternatives` du calcul est fixé à 1 (prévu, non branché).
6. Prêts : 223 par saison, soit environ un par club. Je n'ai pas de référence pour savoir si c'est trop ; le partage salarial minimal du propriétaire est inchangé.
7. Clubs hors périmètre : toujours une revue par semaine hors les 3 derniers jours (sauf poste vacant dans la dernière semaine).
8. **Le vrai binaire de l'application n'a pas été testé** : le manager passif est licencié (mission 8). Le ralentissement de la saison 3 **ne se reproduit pas dans le harnais** (saisons de 19 à 22 s en mode monde entier, 24 à 33 s en mode périmètre actif, sans dérive) ; sa cause reste ouverte pour la mission performance.
9. Trésorerie négative et masse salariale : mêmes ordres de grandeur avant et après sur ce protocole (§ APFM_DEADLINE_DAY_VALIDATION.md), mais **les seuils de la mission 8 (0 club à plus de 150 % à la saison 10) ne sont pas re-mesurés** : 5 clubs à 150 % dès la saison 5 avant mes changements, 7 après en mode monde entier.

## 9. Interface

Aucun changement du frontend. Les écrans existants montrent déjà le prix demandé (contre-offre), le statut « en vente » / « prêt », la durée du contrat et les informations de prêt ; ce sont les mêmes champs que le moteur remplit, avec des valeurs plus justes. Aucun format de sauvegarde n'a changé.

## 10. Vérifications

`cargo fmt --all -- --check` propre ; `cargo clippy -p ofm_core -p season-harness -p db --all-targets` sans avertissement ; `cargo test --workspace --exclude mcp-e2e` : tout passe (ofm_core : 1 634 tests de bibliothèque, 130 tests d'intégration de transferts). 30 tests ajoutés : 13 scénarios, 15 tests unitaires de `realism.rs`, 2 dans `ai_squad_sales.rs`. **Un seul test existant a dû changer** : `ai_clubs_complete_transfer_between_themselves_without_inbox_message` comparait le prix payé à 900 000 £ exacts ; il vérifie maintenant que les deux trésoreries bougent du même montant, que le journal porte ce montant et qu'il reste dans la fourchette bornée (le prix dépend du statut et du contrat). `mcp-e2e` non lancé. Frontend : aucun fichier modifié, tests non relancés.
