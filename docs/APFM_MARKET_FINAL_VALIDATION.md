# AP Football Manager — validation finale du mercato et des finances (mission 12)

29/10/2026. Complète `APFM_ECONOMY_BALANCE_REPORT.md` (analyse et corrections) et `APFM_DEADLINE_DAY_VALIDATION.md` (mission 10). Monde FM24 révision 5. Commits locaux sur `apfm/main`, rien de poussé.

## 1. Protocole

| | |
|---|---|
| Outil | `season-harness/examples/apfm_economy.rs` (jour par jour par les points d'entrée de production, bascule de saison comprise) ; journal des transactions ajouté (`<sortie>.json.deals.jsonl`, une ligne par transfert IA) |
| Monde | `output/m7world/apfm-fm24-2026.json` (révision 5) |
| Graines | **21 et 7**, exécutées l'une après l'autre (jamais en parallèle) |
| Saisons | 5 |
| Périmètre | **monde entier actif** (`--everything`, les 239 clubs IA achètent chaque jour) |
| Club de l'utilisateur | `fm-c-498`, passif |
| Avant | HEAD `453216c1` + seul l'enregistrement (`output/m12/apfm_economy_before`) |
| Après | binaire final du lot (`output/m12/apfm_economy_after`, construit après le dernier changement de code) |
| Dépouillement | `tools/m12_compare.py`, `tools/m12_deals.py` ; données dans `output/m12/{before,after}/` |

Le vrai binaire de l'application n'a **pas** été piloté par MCP dans cette mission (l'outil de la mission 11 `tools/m11_mcp_days.py` existe ; il faut désactiver App Nap, voir le dépôt de mémoire de la mission 11) : la validation longue est celle du harnais. L'application macOS a été construite (§6) mais pas jouée.

## 2. Résultats (moyenne des deux graines, 5 saisons)

| | Avant | Après |
|---|---|---|
| Transferts IA par saison | 403 | 324 |
| Prêts par saison | 545 | 544 |
| Signatures libres par saison | 529 | 541 |
| Part internationale | 52,1 % | 37,3 % |
| Prix payé / valeur (médiane) | 0,78 | 0,80 |
| Transferts sous le prix demandé | 8 % | 29 % |
| Début / milieu / dernière semaine / 72 h / dernier jour (transferts, 10 saisons-graines) | 1 863 / 1 857 / 191 / 60 / 58 | 1 591 / 1 343 / 190 / 72 / 46 |
| Prêts, mêmes phases | 985 / 2 767 / 1 019 / 498 / 185 | 1 046 / 2 668 / 1 027 / 496 / 202 |
| Temps de simulation (5 saisons) | 69,3 s ; 69,6 s | 72,3 s ; 72,2 s (**+4,0 %**) |
| Doublons, joueurs en double, prêts incohérents | 0 | 0 |
| Effectif sénior minimum | 18 | 18 (médiane 23, p90 27) |
| Saisons terminées sans blocage | 2/2 | 2/2 |

Remarque : la mission 10 avait mesuré 305 transferts par saison dans le périmètre actif (3 saisons) ; 324 ici (monde entier, 5 saisons) reste dans la fourchette 200–365 de la mission 8.

### Finances, saison 1 / 3 / 5 (graine 21 | graine 7)

| | Avant, s5 | Après, s5 |
|---|---|---|
| Trésorerie médiane | 68 M | 57 M \| 58 M |
| Trésorerie p90 | 347 M \| 319 M | 293 M \| 300 M |
| Clubs à trésorerie négative | 5,4 % \| 4,2 % | 7,1 % \| 6,7 % |
| Résultat d'exploitation médian | 10,8 M \| 10,5 M | 9,1 M \| 8,2 M |
| Clubs en déficit d'exploitation | 9,6 % \| 10,5 % | 15,1 % \| 14,2 % |
| Charges d'exploitation / revenu | 0 % | 8,3 % |
| Clubs > 110 % / > 150 % de la masse salariale | 51 / 6 \| 45 / 5 | 35 / 5 \| 44 / 2 |

**Dérive financière « extrême »** : aucune (aucun club à trésorerie ou dette sans borne, aucun solde non fini). **Réaliste** : non démontré. La trésorerie médiane reste multipliée par plus de 8 en 5 saisons ; la marge d'exploitation structurelle de 15 % du revenu brut est inscrite dans le monde importé (`importer/finances.py`) et n'est pas corrigée par ce lot.

## 3. Intégrité et légalité

- Aucun doublon de joueur, aucun prêt vers le club propriétaire (compteur `integrity_violations` à 0 à chaque bascule, 2 graines × 7 lectures).
- Aucune transaction hors fenêtre (phase de chaque transfert lue le jour de l'exécution ; le test `nothing_is_signed_with_the_window_closed` vérifie le refus).
- Effectifs viables : minimum de 18 séniors à la fin de la saison 5 dans les deux graines et les deux versions.
- Saison bloquée : aucune (`completed: true`, 5 saisons dans chaque exécution).
- Les trésoreries et propriétaires sont identiques après sauvegarde / rechargement (tests `an_international_transfer_survives_a_save_and_reload`, scénario de prêt I de la mission 10, verts). Aucun format de sauvegarde n'a changé : le signal promotion/relégation est lu dans l'archive d'éditions déjà sauvegardée.

## 4. Scénarios déterministes (phase 7)

| | Scénario | Couvert par | Résultat |
|---|---|---|---|
| A | Club riche, recrutement international | `a_big_english_club_signs_a_clear_upgrade_in_france` | vert |
| B | Petit club, recrutement local | `a_small_french_club_prefers_a_local_solution…`, **nouveau** `a_small_club_does_not_go_abroad_for_a_cheap_marginal_filler` | vert |
| C | Indispensable à contrat long | `an_indispensable_player_is_dearer_two_days_before_the_close`, `the_last_day_does_not_cut_every_price` | vert |
| D | Surplus en fin de mercato | `a_surplus_player_is_cheaper_at_j_minus_2` | vert |
| E | Fin de contrat | `a_player_in_the_last_weeks_of_his_contract_is_cheap_at_j_minus_2`, **nouveau** `a_mainstay_is_kept_but_an_expiring_spare_goes_in_the_last_week` | vert |
| F | Club promu | **nouveau** `ambition_lifts_a_promoted_clubs_valuation…`, `division_move` (budget, signal) | tests de fonction ; **pas de scénario de bout en bout** |
| G | Club relégué | **nouveau** `a_relegated_seller_over_budget_asks_a_little_less`, `a_relegated_club_is_overstretched_at_its_budget…`, `division_move` | idem |
| H | Masse salariale excessive | `a_club_over_its_budget_refuses_a_purchase` | vert |
| I | Trésorerie négative | `is_overstretched_at` (débit compte toujours) ; **nouveau** `calc_upkeep` (rien en dette) | vert |
| J | Prêt de dernière minute | `a_young_bench_player_goes_on_loan_and_comes_back` | vert |
| K | Club sans besoin | `a_club_with_no_need_forces_no_purchase` | vert |
| L | Sauvegarde / chargement pendant le mercato | `an_international_transfer_survives_a_save_and_reload` | vert |

F et G n'ont pas de scénario complet (promotion réelle dans le calendrier, puis mercato) : ils sont couverts par les fonctions pures. C'est la principale lacune de test de ce lot.

## 5. Anomalies restantes

1. **Jour de clôture, monde entier** : non amélioré (46 contre 58). Les besoins sont réglés avant la fin de fenêtre ; la correction réaliste est de créer des besoins tardifs (§5 du rapport d'équilibrage).
2. **Part de clubs à trésorerie négative et en déficit d'exploitation en hausse** (≈ 5 % → 7 % ; ≈ 10 % → 14–15 %) : effet du nouveau poste de charges sur les clubs à faible marge.
3. **Trésorerie médiane encore ×8,4 en 5 saisons** : cause dans l'importeur, non traitée.
4. **Promotion / relégation** : effet non mesuré dans la simulation (le harnais ne journalise pas la division) ; les mouvements entre pyramides par « berth » ne donnent pas de signal.
5. **Acheteurs plus exigeants** (29 % de transferts sous le prix demandé, 8 % avant) : conséquence de l'attente allongée ; le prix médian reste à 0,80.
6. **Mode MCP / vrai binaire** : non rejoué dans ce lot ; **tests frontend** et **`mcp-e2e`** non relancés (aucun fichier frontend modifié).
7. L'utilisateur paie désormais des charges d'exploitation (≈ 8 % de ses revenus pour un club typique, moins s'il a peu de marge) ; `projected_weekly_net` les inclut, l'écran n'a pas de ligne détaillée.

## 6. Build

`npm run tauri build -- --bundles app` : succès (3 min 19 s), `AP Football Manager.app` 26,8 Mo. Lancé 8 s avec un `HOME` isolé : le processus reste vivant et la page appelle le backend. **Non joué, pas de capture d'écran.** `src-tauri/target` : 8,5 Go (un seul dossier), disque libre 37 Go.
