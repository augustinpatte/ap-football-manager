# AP Football Manager — équilibrage économique et mercato (mission 12)

29/10/2026. Fork `~/apfm-migration/openfootmanager`, branche `apfm/main`, **commits locaux, rien de poussé**. `world.db`, `~/apfootballmanager` et `~/apfm-match-lab` non touchés. Monde FM24 inchangé (`WORLD_REVISION = 5`). Aucun format de sauvegarde modifié (aucune colonne, aucune migration).
Mesures : harnais `apfm_economy` (production : mêmes points d'entrée jour par jour), monde `output/m7world`, **2 graines (21 et 7) × 5 saisons × monde entier actif**, club de l'utilisateur passif (`fm-c-498`). Avant = HEAD `453216c1` + seul l'enregistrement des transactions ; après = ce lot. Dépouillement : `~/apfm-migration/tools/m12_compare.py` et `m12_deals.py` ; sorties brutes dans `~/apfm-migration/output/m12/`.

## 1. Résumé honnête

| Phase | Verdict |
|---|---|
| 2 Géographie | **Corrigée** : part internationale 52,1 % → 37,3 %, avec un gradient lisible par taille de club et par valeur du joueur. Aucun quota. |
| 3 Prix | **Diagnostiqué ; correction partielle** : prix/valeur 0,78 → 0,80 (médiane). Ce n'est **pas** un défaut d'acheteur. |
| 4 Jour de clôture, monde entier | **Diagnostiqué ; non amélioré** dans les chiffres (46 transferts le dernier jour sur 10 saisons-graines, contre 58). Voir §5. |
| 5 Promotion / relégation | **Implémentée** (signal explicite, lecture seule). **Effet non mesuré en simulation** : seulement testé en unités. |
| 6 Finances | **Cause identifiée**, charges d'exploitation ajoutées. Trésorerie médiane saison 5 : 68 M → 57 M (−16 %). **Pas résolu** : la marge d'exploitation est inscrite dans le monde importé (§6). Plus de clubs en déficit (§6.4). |

## 2. Transferts internationaux (phase 2)

Mesure sur 4 029 transferts (avant) et 3 242 (après). « Étranger » = pays acheteur ≠ pays vendeur.

| Catégorie | Avant | Après |
|---|---|---|
| Tous | 52,1 % | **37,3 %** |
| Acheteur réputation < 550 (petits) | 41,3 % | **24,5 %** |
| 550–699 | 55,8 % | 37,8 % |
| ≥ 700 (grands) | 63,3 % | 54,2 % |
| Joueur < 0,5 M | 45,7 % | **15,8 %** |
| Joueur 0,5–3 M | 55,7 % | 55,6 % |
| Joueur 3–15 M | 68,9 % | 68,6 % |
| Joueur ≥ 15 M | 67,2 % | 70,3 % |
| Acheteur « poste dégarni » (thin) | 14,0 % | 13,7 % |
| Acheteur France / Angleterre / Espagne / Allemagne / Italie | 41,5 / 59,7 / 50,8 / 63,0 / 61,3 % | 28,9 / 38,2 / 40,7 / 48,0 / 44,7 % |
| Vendeur France / Angleterre / Espagne / Allemagne / Italie | 32,1 / 50,3 / 55,0 / 67,3 / 75,1 % | 18,5 / 37,5 / 44,4 / 50,1 / 55,7 % |

**Biais constaté** : les grands comme les petits clubs achetaient à l'étranger des remplaçants à moins de 500 000 £ (46 % des transferts de joueurs à moins de 500 000 £), alors qu'aucun club ne traverse une frontière pour un joueur de banc qui ne l'améliore guère. Le poids géographique de la mission 10 ne suffisait pas : un joueur étranger n'entrait en concurrence que lorsque aucun joueur national n'était acceptable, et il le devenait trop souvent.

**Corrections** (`transfers/realism.rs`, `market.rs`), toutes calculées et bornées :
1. `worth_the_border` : un joueur étranger de moins de 500 000 £ n'est considéré que s'il améliore la première doublure d'au moins 4 points, ou si le poste est dégarni. Un joueur du pays n'est jamais refusé ici.
2. `foreign_visibility` : le poids étranger de base est multiplié de 0,6 (≤ 300 000 £) à 1,0 (≥ 3 M£), échelle logarithmique.

Ce qui n'a **pas** été touché : un club anglais riche recrute toujours largement à l'étranger (scénario A de la mission 10, vert) ; un joueur de 15 M£ et plus attire 70 % d'acheteurs étrangers, de plusieurs pays.

## 3. Prix de transfert (phase 3)

La question : la médiane 0,78 vient-elle du vendeur ou de l'acheteur ? **Du vendeur.** Avant, l'acheteur payait le prix demandé dans 92 % des cas (332 transferts sur 4 029 conclus en dessous) ; prix demandé / valeur = prix payé / valeur = 0,78. Après, 29 % se concluent sous le prix demandé (943 sur 3 242, d'au plus 8 %) : l'attente plus longue des acheteurs du lot a donc **rendu l'acheteur plus exigeant**, ce qui retient un peu le prix (0,81 demandé, 0,80 payé).

La médiane est un effet de composition (avant, mêmes ordres de grandeur) :
- 52 % des ventes sont des **surplus**, 11 % des **indésirables**, 10 % des remplaçants ; 57 % des contrats ont **moins d'un an** (avant : 55 %) et 4 % moins de 60 jours (prix 0,53) ;
- une remise de mise en liste (−0,10) **s'ajoutait** à la remise de statut (surplus −0,12, indésirable −0,18) pour la même raison : le club veut s'en séparer. Un surplus listé partait à 0,78 sans autre cause.

**Corrections** (`asking_multiplier`) :
- les deux remises ne se cumulent plus : la plus forte s'applique en entier, la moitié de l'autre ;
- la remise de surplus (−0,06) et d'indésirable (−0,10) est faible un jour ordinaire et **croît avec la fermeture** (−0,26 et −0,36 × pression) : un surplus avec des années de contrat n'est plus bradé en début de fenêtre ;
- un club relégué dont la masse salariale dépasse son budget demande 4 points de moins (§4) ;
- l'attente de l'acheteur passe de 0,15 / 0,08 (début / milieu) à 0,18 / 0,12 : elle retient les achats non urgents (voir ci-dessus : 29 % de transferts sous le prix demandé).

Prix payé / valeur (médiane), après :

| Catégorie | Avant | Après |
|---|---|---|
| Tous | 0,78 | 0,80 |
| Surplus | 0,78 | 0,83 |
| Indésirable | 0,62 | 0,73 |
| Contrat > 1 an et < 3 ans | 0,80 | 0,84 |
| Contrat ≥ 3 ans | 0,82 | 0,87 |
| Contrat ≤ 60 jours | 0,50 | 0,53 |
| Titulaire / indispensable | 1 titulaire à 0,86 ; 0 indispensable | 3 titulaires à 0,91 ; 0 indispensable |
| Dernier jour | 0,71 | 0,66 |
| Dernières 72 h | 0,76 | 0,73 |

Le dernier jour est plus bas (0,66) **parce que** ceux qui sont encore à vendre ce jour-là sont des surplus et des fins de contrat, pas parce que tout baisse : les titulaires et les indispensables ne baissent pas à l'approche de la clôture (test `the_last_day_does_not_cut_every_price`). **Aucun joueur indispensable n'a été vendu** dans les 3 242 transferts, et les 3 titulaires l'ont été à 0,91.

Négociations et contre-offres : les 109 tests de transferts (scénarios A à L de la mission 10 + 2 nouveaux) passent ; le prix demandé, le plafond de l'acheteur et le prix plancher d'une offre au joueur de l'utilisateur sont inchangés.

## 4. Promotion et relégation (phase 5)

**Signal.** `division_move.rs` lit l'archive des éditions (déjà sauvegardée) : les clubs promus ou relégués par les frontières de la dernière saison archivée. Rien n'est stocké (pas de colonne, pas de migration, rechargement identique par construction). Les transferts de barrage par « berth » entre pyramides n'ont pas d'enregistrement et ne donnent pas de signal.

Effets, tous par les mécanismes existants :
| | Promu | Relégué |
|---|---|---|
| Budget salarial (une fois, à la bascule) | +12 % au plus, vers 75 % du budget médian de sa nouvelle division, jamais s'il est à découvert | −12 % au plus, vers 140 % du budget médian de la division, jamais de hausse |
| Marché, acheteur | n'attend pas le début de la fenêtre ; valorise +0,04 ; regarde 50 points de réputation plus haut | se déclare « surétendu » dès 100 % du budget (120 % ailleurs) |
| Marché, vendeur | — | « en difficulté » dès 90 % du budget : ses salaires coûteux deviennent des indésirables (donc prêtés ou vendus) ; demande 4 points de moins s'il dépasse son budget |

Aucune vente ni aucun achat n'est forcé. Le club de l'utilisateur n'est pas touché.
**Limite** : je n'ai pas mesuré dans la simulation le comportement des promus et relégués (le harnais n'enregistre pas la division). Vérifié : 2 tests de la fonction de budget, 1 test de lecture du signal, 1 de la surétendue, 1 de l'ambition, 1 de la remise de relégation.

## 5. Jour de clôture, monde entier (phase 4)

Transferts IA, 5 saisons × 2 graines, monde entier actif :

| Phase | Avant | Après |
|---|---|---|
| Début (> 40 j) | 1 863 | 1 591 |
| Milieu | 1 857 | 1 343 |
| Dernière semaine | 191 | 190 |
| 72 dernières heures | 60 | 72 |
| Dernier jour | 58 | 46 |
| Prêts, dernière semaine / 72 h / dernier jour | 1 019 / 498 / 185 | 1 027 / 496 / 202 |

**Pourquoi le monde entier est peu actif en fin de fenêtre.** Dans ce mode les 239 clubs achètent chaque jour dès le 21ᵉ jour (35 % attendent les 20 premiers). Un achat exige un **besoin** (poste dégarni ou joueur meilleur que la première doublure), un vendeur **disposé** et un budget. Le prix n'est pas la contrainte (29 % seulement des ventes se concluent sous le prix demandé) ; ce sont les besoins, et ils sont réglés dans les 33 jours du milieu de fenêtre. Il n'en naît presque aucun de nouveau à la fin (pas de blessure longue ni de départ qui crée un trou à cette date). Le mode « périmètre actif » (par défaut du jeu) garde la courbe en U de la mission 10 parce que les clubs hors périmètre n'achètent qu'un jour sur sept.

**Ce que j'ai ajouté** : les joueurs dont le contrat finit dans moins de 60 jours et qui ne sont pas titulaires sont vendables dès la dernière semaine (« un club prend une indemnité plutôt que de les perdre pour rien ») ; la remise des surplus croît avec la pression ; l'attente des acheteurs est un peu plus longue. **Effet mesuré : nul à la clôture** (46 contre 58, 72 contre 60 : dans le bruit), le milieu de fenêtre perd 28 % de ses transferts. **Je ne prétends pas que le jour de clôture est maintenant plus dynamique en monde entier** ; la cause est structurelle (des besoins réglés trop tôt) et le correctif réaliste serait de **créer des besoins tardifs** : un trou ouvert par la vente d'un titulaire, une blessure longue pendant la fenêtre, l'intérêt d'un grand club pour un titulaire du vendeur. Recommandé pour la mission suivante.

## 6. Finances structurelles (phase 6)

### 6.1 Causes réelles (mesurées, graine 21, état avant)

Trésorerie médiane : 6,8 M → 18 M → 33 M → 45 M → 58 M → 68 M ; p90 72 M → 347 M. Revenu des 239 clubs ≈ 12,6 Md€ par saison, salaires 6,5 Md€ + 0,57 Md€ staff : **les salaires pèsent 55–60 % des revenus et le résultat d'exploitation médian est de +14 M€ par club et par saison**, stable. Les transferts sont à somme nulle (frais payés = frais reçus). Les salaires de la population ne montent pas (127 M€ par semaine pour les clubs IA à la saison 5, contre 136 M€ à l'ouverture) et le budget salarial n'est rempli qu'à 88 %.

Origine de cette marge : `importer/finances.py` fixe `marge_exploitation = 15 %` **du revenu brut** ; « autres charges » est le solde `revenu × 0,85 − salaires`, retranché du revenu récurrent avant l'import (`fm24-adapter/finance.rs` : *TV + autres revenus − autres charges − recette billetterie estimée*). Le jeu garde donc **15 % du revenu brut chaque année, pour toujours**, sans salaire qui rattrape le revenu et sans aucun poste de dépense qui existe dans le jeu (`calc_upkeep` valait 0 ; `CashKind::Upkeep` n'était jamais alimenté). Une marge de 15 % est bien au-dessus de la réalité des clubs européens.

### 6.2 Ce qui a été ajouté — et pourquoi pas davantage

`finances::calc_upkeep(team, salaires, revenu)` alimente enfin `CashKind::Upkeep` chaque semaine :

| Poste | Formule | Dépend de |
|---|---|---|
| Stade | 25 % d'une recette de stade pleine, par semaine | capacité |
| Installations | 1,5 % de la masse salariale par niveau (entraînement + médical + recrutement) | niveaux, taille du club |
| Administration | 6 % de la masse salariale (joueurs + staff) | taille du club |

**Garde-fous** (pour ne pas prélever deux fois et préserver les clubs fragiles) :
- plafond : **35 % de ce qui reste après les salaires** (revenu récurrent + recette estimée − salaires) ; un club dont les salaires dépassent le revenu ne paie rien ;
- proportionnel à la réserve : en dessous de 13 semaines de salaires en caisse, le club ne paie qu'une fraction, rien s'il est à découvert ;
- les salaires de staff sont déjà dans `calc_wages` : le poste « administration » ne les compte qu'une fois (corrigé en cours de lot : le snapshot les comptait deux fois) ;
- les académies et le staff ne reçoivent **aucun poste nouveau** : leurs coûts sont déjà dans les salaires.

La **double-comptabilité** reste un risque : le revenu importé est déjà net de la ligne « autres charges » de l'importeur, qui couvre en principe le même genre de coûts. J'ai donc plafonné ces nouveaux postes à une fraction de la marge restante et je les présente comme **prélevés sur la marge de 15 %**, non comme un coût en plus. Le vrai remède est dans l'importeur (marge, croissance des salaires), qui n'est pas dans le périmètre de cette mission.

### 6.3 Résultat (2 graines, médianes entre graines)

| Saison 5 | Avant | Après |
|---|---|---|
| Trésorerie médiane | 68 M | **57–58 M** |
| Trésorerie p90 | 319–347 M | 293–300 M |
| Résultat d'exploitation médian | 10,5–10,8 M | 8,2–9,1 M |
| Charges d'exploitation / revenu | 0 % | 8,2 % |
| Salaires / revenu | 55–57 % | 55–56 % |
| Clubs à trésorerie négative | 4,2–5,4 % | **6,7–7,1 %** |
| Clubs en déficit d'exploitation | 9,6–10,5 % | **14,2–15,1 %** |
| Clubs à plus de 110 % / 150 % du budget salarial | 45–51 / 5–6 | 35–44 / 2–5 |

### 6.4 Ce que ces chiffres disent — et ne disent pas

- La trésorerie cumulée ralentit de 16 % : **la structure reste excédentaire** (médiane 57 M à la saison 5, soit ×8,4 en 5 saisons). Je ne déclare **pas** les finances réalistes.
- La part de clubs à trésorerie négative **monte** (≈ 5 → 7 %) et celle des clubs en déficit d'exploitation aussi (≈ 10 → 14–15 %) : les charges pèsent plus sur les clubs peu rentables que sur les riches (leur part de revenu est plus élevée), malgré le plafond et la réserve. C'est une régression du point de vue « préserver les clubs fragiles ». Une piste : un plafond plus bas ou une échelle selon la division.
- Le club de l'utilisateur paie les mêmes charges ; `projected_weekly_net` les inclut (aucune ligne détaillée dans l'écran : les textes n'ont pas changé).
- Aucun poste n'a été ajouté pour réduire un solde sans mécanisme ; mais la valeur de chaque taux (25 %, 1,5 %, 6 %) est une estimation d'ordre de grandeur, sans source externe.

## 7. Fichiers modifiés

`ofm_core/src/transfers/{realism,market,mod}.rs` (prix, géographie, vente, signal), `ofm_core/src/division_move.rs` (nouveau), `ofm_core/src/end_of_season/mod.rs` (appel de re-basage), `ofm_core/src/finances/mod.rs` (charges), `ofm_core/src/lib.rs`, `season-harness/examples/apfm_economy.rs` (journal des transactions), tests : `ofm_core/tests/{finances_tests,transfers_tests/realism}.rs`.

## 8. Tests et build

`cargo test --workspace --exclude mcp-e2e --no-fail-fast` : **3 924 réussis, 0 échec, 33 ignorés** (journal : `~/apfm-migration/cargo_test_m12.log`). `cargo fmt --all -- --check` propre ; `cargo clippy -p ofm_core -p season-harness -p db --all-targets` sans avertissement. Nouveaux tests : 8 dans `transfers/realism.rs` (prix sans double remise, jour de clôture, remise du relégué, visibilité étrangère, ambition du promu, frontière, vendeur disposé en fin de contrat, surétendue), 3 dans `division_move.rs` (re-basage promu/relégué, lecture du signal), 1 dans `finances` (charges), 1 scénario de transfert (`tests/transfers_tests/realism.rs`, M12-A) ; 2 tests existants de `finances_tests.rs` mis à jour (les charges d'exploitation existent maintenant). Frontend : aucun fichier modifié, **tests non relancés**. `mcp-e2e` non lancé.
