# AP Football Manager — document de passation (Codex)

29/10/2026, fin de la mission 12. Rédigé à partir du code et des mesures de cette session, pas seulement des rapports antérieurs. Ce qui n'a pas été vérifié est dit comme tel. Langue des échanges avec le propriétaire : **français**. Le propriétaire ne code pas : tout est fait par l'agent et expliqué en français.

## 1. Objectif du jeu

Jeu de management de football pour **macOS**, dans l'esprit de Football Manager 24, **local** (aucun service en ligne), construit sur **OpenFoot Manager** (GPL-3.0). Il charge le monde FM24 2026/27 du propriétaire (240 clubs, 12 divisions, 5 pays : Angleterre, France, Espagne, Italie, Allemagne), le simule saison après saison (matchs, calendrier, coupes nationales, promotions et relégations, marché des transferts, contrats, finances, staff et délégation, entraînement et académie), avec des sauvegardes multi-saisons.

## 2. Architecture réelle

- **Tauri v2** (coque) + **Rust** (cœur) + **React 19 / TypeScript / Tailwind v4** (interface). `docs/ARCHITECTURE.md` est le document de référence d'OFM.
- Crates de `src-tauri/crates/` : `domain` (types), `engine` (moteur de match, **n'importe jamais `domain`** : `ofm_core/turn` est le pont), `ofm_core` (toute la logique du jeu), `db` (SQLite, migrations), `fm24-adapter` (import du monde FM24, binaire `apfm-import`), `season-harness` (banc de simulation sans interface), `mcp-results`, `mcp-e2e`, `ofm-cli`, `sim-bench`.
- Application : `src-tauri/src` (commandes Tauri, partagées avec un serveur MCP derrière la fonctionnalité `mcp`), interface : `src/`.
- Le marché IA : `ofm_core/src/transfers/` (`market.rs` balayage quotidien ; `realism.rs` fonctions pures : statut, prix demandé, plafond de l'acheteur, géographie, phases de la fenêtre ; `lifecycle.rs` ; `loans.rs`). Signal promotion/relégation : `ofm_core/src/division_move.rs`. Finances : `ofm_core/src/finances/mod.rs`, révision annuelle des budgets salariaux `ai_budget_review.rs`, bascule de saison `end_of_season/mod.rs`.

## 3. Structure du dépôt

`~/apfm-migration/openfootmanager` (clone d'OFM, **dépôt git ; `origin` pointe vers le dépôt public d'OFM : ne jamais pousser**). Dossiers utiles : `src-tauri/` (Rust), `src/` (React), `docs/` (documents d'OFM + `APFM_*.md`), `scripts/` (dont `scripts/apfm/` : dépouillement de la mission 12), `AGENTS.md` / `CLAUDE.md` (règles de contribution : tests d'abord, toutes les langues, `#[serde(default)]` sur tout nouveau champ sérialisé, commits conventionnels).
Hors dépôt, dans `~/apfm-migration/` : `docs/` (rapports des missions 1 à 11), `tools/` (scripts Python des missions : `mcp_*`, `m*_compare.py`), `output/` (mondes générés, résultats de mesure ; `output/m7world` = monde de référence actuel, `output/m12/` = mesures de cette mission), `*.log`.

## 4. Commandes de développement

```bash
cd ~/apfm-migration/openfootmanager
export PATH="/opt/homebrew/opt/node@24/bin:$HOME/.cargo/bin:$PATH"   # Node 24 (le 26 casse un test), cargo via rustup
npm ci                                  # une fois ; ensuite npm run <script>, jamais npx biome
npm run tauri dev                       # application complète
cargo build --release -p season-harness --example apfm_economy   # banc d'économie
```

## 5. Commandes de tests

```bash
cd ~/apfm-migration/openfootmanager/src-tauri
cargo test --workspace --exclude mcp-e2e --no-fail-fast   # 3 924 réussis, 0 échec, 33 ignorés (mission 12)
cargo test -p ofm_core --lib -- transfers:: finances:: division_move         # ciblé
cargo test -p ofm_core --test transfers_tests --test finances_tests
cargo fmt --all -- --check
cargo clippy -p ofm_core -p season-harness -p db --all-targets
cd .. && npm test && npm run build       # frontend (non relancé en mission 12 : aucun fichier modifié)
```
Avec le monde FM24 : `APFM_WORLD_JSON=$HOME/apfm-migration/output/m7world/apfm-fm24-2026.json cargo test --release -p season-harness --test fm24_competitions`.
Mesure du marché (une graine, 5 saisons, monde entier) : `APFM_WORLD_JSON=… target/release/examples/apfm_economy fm-c-498 5 sortie.json --everything --seed 21` ; dépouillement : `scripts/apfm/m12_compare.py <dossier>` et `m12_deals.py <dossier>` (`scripts/apfm/m12_run.sh` enchaîne deux graines **l'une après l'autre**).

## 6. Build macOS

```bash
cd ~/apfm-migration/openfootmanager
export PATH="/opt/homebrew/opt/node@24/bin:$HOME/.cargo/bin:$PATH"
npm run tauri build -- --bundles app     # ≈ 4 à 8 min
```
Livrable : `src-tauri/target/release/bundle/macos/AP Football Manager.app` (26,8 Mo, signature ad hoc, arm64). **Construit à la fin de la mission 12** (3 min 19 s de compilation) ; lancé 8 s avec un `HOME` isolé : le processus reste vivant, la page appelle le backend (`[startup] … 453216c`, `get_settings`). Il n'a pas été **joué** (ni capture d'écran). **Un seul dossier `target`** (`src-tauri/target`, ≈ 8 Go) : ne pas en créer un second. Profils : `debug = 0`, `incremental = false` (dev et test) ; release en LTO fin, 1 unité de génération. Mode MCP (jeu piloté sans fenêtre) : `--features mcp --no-gui`, **désactiver App Nap** (voir §13).

## 7. Gestion des sauvegardes

Données de l'application : `~/Library/Application Support/com.apfootballmanager.app/` (`databases/` = monde installé, `fm24-install.json` = marqueur de révision, les sauvegardes). **Un fichier `<uuid>.db` par carrière** ; un nouveau jeu n'écrase jamais l'existant. `APFM_SAVES_DIR` (chemin absolu) déplace les sauvegardes (préparation de la synchronisation entre deux Mac, non mise en œuvre). Règle d'or du lot : **ne pas ajouter de colonne aux sauvegardes sans migration** (`db/src/migrations.rs` contient des empreintes ; `#[serde(default)]` ne suffit pas pour les champs qui atteignent SQLite — `src-tauri/CLAUDE.md` §2). La mission 12 n'a modifié aucun format.

## 8. Import FM24

Source : `~/apfootballmanager/data/world.db` (produit par l'importeur Python de l'autre dépôt ; **lecture seule, jamais modifié**). Passage : `fm24-adapter` (`apfm-import <world.db> <dossier>`, `--verify-determinism`) → monde natif JSON (`apfm-fm24-2026.json`) + rapports, contrôles d'intégrité (refus d'écrire si un contrôle bloquant échoue). Installation dans l'application : `src-tauri/src/commands/fm24_world.rs`, détection d'un monde obsolète par `WORLD_REVISION`. Détection automatique de `world.db` : `APFM_WORLD_DB`, `<données>/fm24/world.db`, `~/apfootballmanager/data/world.db`.

## 9. WORLD_REVISION actuel

**5** (`src-tauri/crates/fm24-adapter/src/install.rs:18`). Le monde de référence des mesures est `~/apfm-migration/output/m7world/apfm-fm24-2026.json`. Un changement du monde (règles de compétition, finances de départ, effectifs) impose d'incrémenter la révision.

## 10. Systèmes déjà fonctionnels

240 clubs, 12 divisions, règles de montée/descente et barrages écrites par championnat, 6 coupes nationales, effectifs suivant l'inscription FM24 (prêts, équipes B), marché mondial IA (statut du joueur, prix demandé / plafond de l'acheteur, géographie, fenêtre de transfert et jour de clôture, prêts), contrats et salaires, finances (revenu récurrent, billetterie, charges d'exploitation depuis la mission 12), révision annuelle du budget salarial, staff, délégation et adjoint, entraînement et académie, sauvegardes multi-saisons, 12 langues, App Nap désactivé pendant la simulation. Moteur de match 2D inchangé depuis la mission 5.

## 11. Limitations connues

- Compétitions : pas de coupe de la Ligue (hors EFL Cup), pas de coupes européennes, calendrier d'OFM (pas celui de FM24) ; bas de pyramide sans relégation ; barrages simulés à la bascule, non joués (`APFM_REMAINING_GAPS.md`).
- Marché : voir §12.
- Aucun visuel de l'application n'a été validé par capture (pas d'outil d'écran disponible lors des missions précédentes).

## 12. Bugs et anomalies prioritaires (état mesuré, mission 12)

1. **Finances trop favorables** : trésorerie médiane ×8,4 en 5 saisons (68 M → 57 M après le lot ; était ×10). Cause : l'importeur (`~/apfootballmanager/importer/finances.py`, `marge_exploitation = 0,15`) fixe une marge de 15 % du revenu brut, perpétuelle, sans croissance des salaires. Remède = importeur (+ `WORLD_REVISION`), pas le moteur.
2. **Plus de clubs en difficulté après les charges d'exploitation** : trésorerie négative ≈ 5 → 7 %, déficit d'exploitation ≈ 10 → 14–15 % (saison 5). À corriger : plafond plus bas ou barème par division (`calc_upkeep`, `UPKEEP_SHARE_OF_MARGIN`, `RESERVE_WEEKS`).
3. **Jour de clôture, monde entier** : 46 transferts sur 10 saisons-graines (58 avant) ; les besoins sont réglés avant la dernière semaine. Il faut créer des besoins tardifs (trou laissé par une vente de titulaire, blessure longue, intérêt d'un grand club pour un titulaire).
4. **Promotion / relégation** : le signal et le re-basage du budget existent (`division_move.rs`) mais **leur effet n'a pas été mesuré en simulation** ; pas de scénario de bout en bout ; les mouvements par « berth » ne donnent pas de signal.
5. **Acheteurs plus exigeants** (29 % de transferts sous le prix demandé, 8 % avant) depuis l'attente allongée ; prix/valeur médian 0,80 (composition : surplus et fins de contrat).
6. Les joueurs indispensables ne sont presque jamais vendus (0 sur 3 242) : le champ de prix « indispensable » n'est visible que dans les offres adressées à l'utilisateur.
7. Le **vrai binaire** n'a pas été rejoué par MCP dans la mission 12 ; les tests frontend et `mcp-e2e` non plus.
8. `calc_upkeep` s'applique aussi au club de l'utilisateur ; `projected_weekly_net` l'inclut, mais l'écran de finances n'a pas de ligne dédiée.

## 13. Risques de performance

Harnais : 14 s par saison en monde entier (72 s pour 5 saisons, +4 % avec ce lot ; jour médian ≈ 32 ms). **Le « ralentissement de la saison 3 » du vrai binaire était App Nap** (processus sans fenêtre : priorité réduite ×8–11 dès ~30–60 s après le lancement) : toute mesure en mode MCP doit lancer l'application avec `NSAppSleepDisabled` (voir `docs` de la mission 11, `tools/m11_mcp_days.py`) ; ne pas chercher d'algorithme lent pour ce symptôme. Le balayage du marché est O(joueurs + clubs × liste restreinte) ; garder cette propriété.

## 14. Règles de gestion du stockage

Disque : 228 Go, 37 Go libres en fin de mission. Un seul `src-tauri/target` (≈ 8 Go ; ne pas dupliquer, ne pas ajouter de `debug = true` ni de compilation incrémentale — le dossier a atteint 47 Go avant la mission 7). Sorties de mesure dans `~/apfm-migration/output/` (le dossier `m12` pèse quelques Mo). Surveiller `df -h /` avant chaque build ; `cargo clean -p <crate>` plutôt que `cargo clean`.

## 15. Contraintes de licence

GPL-3.0 (`LICENSE.md`, inchangé). Copie modifiée d'OpenFoot Manager (`NOTICE-APFM.md`). **Les données de Football Manager (SI / SEGA) ne sont ni fournies ni distribuées** : elles sont lues dans la sauvegarde du propriétaire et restent sur sa machine. Tests de garde : `src-tauri/tests/distribution_guard.rs` (aucune base ni monde FM24 dans le bundle ni dans git). Aucune donnée n'est envoyée au projet d'origine.

## 16. Fichiers et dossiers à ne jamais modifier

- `~/apfootballmanager` (dépôt de l'importeur), **`~/apfootballmanager/data/world.db`** et `data/raw/` ;
- `~/apfm-match-lab` (laboratoire du moteur de match) ;
- le monde installé et les sauvegardes du propriétaire (`~/Library/Application Support/com.apfootballmanager.app/`) ;
- `LICENSE.md`, et les résultats de mesure historiques `~/apfm-migration/output/m*` (copier, ne pas écraser).
Ne jamais commiter de données FM24 (le `.gitignore` et `distribution_guard.rs` le vérifient).

## 17. État de Git

Dépôt `~/apfm-migration/openfootmanager`, branche **`apfm/main`**, branche d'origine `develop@f908c2e5`. Aucun `push` n'a jamais été fait (`origin` = dépôt public d'OFM : **ne pas pousser sans demande explicite**). Arbre propre à la livraison de la mission 12 (hors dossier `target` et `node_modules`, ignorés).

## 18. Derniers commits

Voir `git log --oneline | head` : les commits de la mission 12 sont, du plus ancien au plus récent, `61cf7bdc` (marché : frontière, prix, signal promotion/relégation), `b183465f` (charges d'exploitation), puis les documents et scripts de la mission. Missions précédentes : `453216c1` (App Nap), `ad307bf3` (mercato réaliste), staff (`7a0d046a` à `bd00613b`), compétitions (`7588908e` à `4e242b22`), marché mondial (`bd73a9cd` à `67f8f974`).

## 19. Prochaines fonctionnalités recommandées (dans l'ordre)

1. Corriger la marge structurelle dans l'importeur (marge d'exploitation moins élevée, croissance des salaires avec le revenu) puis incrémenter `WORLD_REVISION` ; alors seulement ré-étalonner `calc_upkeep` (voir §12.1–2).
2. Besoins tardifs de recrutement (jour de clôture) et mesure de l'effet des promotions / relégations dans le harnais (journaliser la division).
3. Jouer le vrai binaire (MCP, App Nap désactivé) sur 5 saisons, sauvegarde/rechargement en plein mercato, puis un parcours manuel de 5 minutes à l'écran (protocole du rapport de la mission 5, §9).
4. Ligne « charges d'exploitation » dans l'écran de finances (les 12 langues).
5. Coupe de la Ligue / coupes européennes (aujourd'hui absentes).

## 20. Protocole de validation après chaque mission

1. `git status` propre avant et après ; commits locaux conventionnels, **aucun push**.
2. `cargo fmt --all -- --check`, `cargo clippy -p ofm_core -p season-harness -p db --all-targets` sans avertissement.
3. `cargo test --workspace --exclude mcp-e2e --no-fail-fast` : nombre de réussis / échecs / ignorés noté ; tout test existant modifié est justifié dans le rapport.
4. Si le frontend change : `npm test`, `npm run build`, tests de langues (`src/i18n`).
5. Mesure avant/après avec `apfm_economy` : **deux graines, cinq saisons, monde entier, l'une après l'autre**, binaire « avant » gelé ; contrôles : doublons 0, prêts incohérents 0, effectif sénior minimum ≥ 18, aucune saison bloquée.
6. Jamais « réaliste » parce qu'un chiffre baisse : donner la composition (par statut, durée de contrat, taille de club) et dire ce qui n'est pas mesuré.
7. Si la sauvegarde ou le monde change : migration + `WORLD_REVISION` + test de rechargement identique.
8. `npm run tauri build -- --bundles app`, taille de `target` et espace disque libre dans le rapport.
9. Rapport dans `docs/APFM_*.md` : résultats avant/après, anomalies restantes, ce qui n'a pas été testé.
10. Vérifier que `~/apfootballmanager`, `world.db` et `~/apfm-match-lab` n'ont pas été touchés (dates de modification).
