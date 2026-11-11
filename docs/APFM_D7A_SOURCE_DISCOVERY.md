# APFM D7A — Source discovery: the FM24 database on disk

**Status:** read-only audit. **Date:** 2026-11-11. **Branch:** `deepseek/d7-database-audit`.
**Scope:** establish what the FM24 download actually is, what it contains, how it was turned into
`world.db`, and what the Rust adapter already reads from it. No original file was moved, renamed,
overwritten or deleted. No gameplay, engine, M12 or frontend code was changed.

Companion document: [`APFM_D7A_IMPORT_READINESS.md`](APFM_D7A_IMPORT_READINESS.md) — blockers,
architecture assessment, D7B plan and GO/NO-GO.

---

## 1. Artifacts found

The download is two files. Neither is an FM editor database, and neither is a complete
FM25/FM26/FM27 database: both are labelled FM24 and were produced for the 2026/27 season by a
third-party update.

| Path | Size (bytes) | Modified | SHA-256 | What it is |
|---|---:|---|---|---|
| `~/football-manager/2026-27 SEASON FREE SAVE GAME.fm` | 125,695,178 | 2026-10-26 10:57 | `70915fb59a6209e0b1a76789fe0ae912fac4aac9347a123b63350478ec018d4e` | FM24 **save game** (FMF container) |
| `~/football-manager/CUSTOM GRAPHICS- FM24 UPDATE 2026-27.zip` | 21,946,051 | 2026-10-26 10:58 | `1ba481590e90f5c457bcb2caa0f5c1b0e95f8916652040104a103c1150b67dc6` | Graphics pack only (258 files, PNG + `config.xml`) |
| `~/apfootballmanager/data/world.db` | 241,463,296 | 2026-10-27 05:41 | `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` | **Derived** SQLite database, built from the `.fm` save |

`~/apfootballmanager/data/` also contains `raw/` and `work/`; `world.db` is the only derived
database used below.

### 1.1 Format evidence

* **`.fm`** starts with the bytes `02 01 66 6d 66 2e` (`\x02\x01fmf.`), the FMF container magic
  read by the importer's `save.py` via the `fmsave` package. It is a compressed, framed container
  holding named sections (`game_db`, `game_info`, `person_db_changes`, …). It is **not** a plain
  SQLite or CSV export, and it is **not** the FM editor's data file.
* **The zip** contains only graphics (`000 - FM24 UPDATE 2026-27/...` PNG faces/logos and a
  `config.xml`). `unzip -l` reports 258 entries and no `.dbc`, `.edt`, `.sqlite` or `.db` payload.
* **`world.db`** is SQLite. Its `import_run` row records the source file name and hash, the game
  version, the importer version and the import timestamp:

  | Column | Value |
  |---|---|
  | `source_file` | `2026-27 SEASON FREE SAVE GAME.fm` |
  | `source_sha256` | `70915fb5…018d4e` |
  | `game_version` | `24.3.0` |
  | `start_season` | `2026` |
  | `importer_version` | `1.1.0` |
  | `imported_at` | `2026-10-26T15:57:19Z` |

  The importer's `__init__.py` declares `IMPORTER_VERSION = "1.1.0"` and `SCHEMA_VERSION = 3`.

### 1.2 Provenance caveats

The public page that distributes this update now describes it as *“FM24 Season 2026/27 — NOVEMBER
2026”*, and its comments state that the free download is a **save game** while the database file
is a paid tier. The save itself proves the FM build (`24.3.0`) and the start season (`2026`); it
does **not** prove which release tier, patch level or editorial pass produced it. Treat the
provenance as *third-party FM24 2026/27 save, exact tier unverifiable from the file alone*.

---

## 2. `world.db` schema and volume

The importer writes a normalised SQLite schema (v3). 35 tables, grouped here by concern:

| Group | Tables |
|---|---|
| People | `person`, `person_personality`, `player`, `player_attributes`, `player_position`, `staff`, `staff_attribute`, `staff_preference`, `staff_role`, `staff_unplaced_raw` |
| Clubs and teams | `club`, `club_colours`, `club_facilities`, `club_finance_start`, `team` |
| Competitions | `competition`, `competition_season`, `competition_season_team`, `competition_rule`, `competition_target`, `league_rules`, `league_tv_rights`, `cup_rules`, `stage`, `qualification`, `fixture` |
| Contracts and money | `contract`, `contract_clause`, `clause_kind`, `value_model` |
| Reference | `nation`, `attribute_ref`, `asset`, `data_check`, `import_run` |

Verified row counts (`SELECT count(*)`, 2026-11-11):

| Entity | Rows |
|---|---:|
| Persons | 127,192 |
| **Players** | **81,762** |
| Staff | 45,430 |
| Clubs | 44,032 |
| Teams (senior, reserve, youth) | 106,779 |
| Nations | 228 |
| Competitions | 1,585 |
| Competition seasons (all 2026) | 179 |
| Competition membership rows (2026) | 2,879 |
| Contracts | 89,177 |
| Current contracts (`is_current = 1`) | 89,140 |
| Contract clauses | 190,321 |
| Player-position rows | 1,226,430 |
| Fixtures | 28,888 |
| Player attributes | 81,762 |

### 2.1 Is the “~80,000 players” figure real?

**Yes.** `player` holds **81,762** distinct `person_id` rows. Every one parses from the save.
The figure the download advertises is therefore accurate for the *source database as a whole*.
It is **not** the size of the playable world shipped today: see §5.

---

## 3. Coverage and data quality

### 3.1 Competitions

| `kind` | Count |
|---|---:|
| `league` | 99 |
| `cup` | 6 |
| `continental` | 3 |
| `other` | 1,477 |
| **Total** | **1,585** |

Only 21 competitions carry `playable = 1` (19) or `simulated = 1` (2):

| Scope | Competitions |
|---|---|
| Playable leagues (10) | Premier League, Championship, League One, Ligue 1, Ligue 2, Bundesliga, Serie A, Serie B, LaLiga, LaLiga 2 |
| Simulated leagues (2) | French National (Ligue 3), 2. Bundesliga |
| Playable cups (6) | Coupe de France, DFB-Pokal, Coppa Italia, Copa del Rey, FA Cup, League Cup |
| Playable continental (3) | Champions League, Europa League, Conference League |

The 99 `league` rows are the ones with 2026 membership in `competition_season`; **86 of them have
no `competition_target` row**, so they have no name, nation, pyramid level or target scope in the
adapter's reference data. Membership is thin and overlapping: the 99 leagues hold **1,570**
membership rows for **1,184 distinct clubs** (some clubs appear in two divisions). **42,848**
clubs belong to no 2026 league at all.

`competition_target.scope` uses `jouable` / `simulée` / `représentée` / `absente`; only the first
two feed the adapter perimeter.

### 3.2 Players by contract scope

Partition of the 81,762 players by their **current contract's club** (sums exactly to 81,762):

| Where the player is contracted | Players |
|---|---:|
| One of the 240 clubs of the 12 playable/simulated leagues | **9,964** |
| A club of another 2026 league (944 clubs in 87 leagues) | 21,822 |
| A club with no 2026 league membership (42,848 clubs) | 29,528 |
| **No current contract** | **20,448** |

A different view, by the team the save registers the player with, gives **8,689** players
registered inside the 240-club perimeter and **20,481** with no registered team at all. The two
views differ for loans and B/academy sides; the adapter uses the contract club and the registered
team together (§7.3).

### 3.3 Missing and unreliable fields

| Fact | Count |
|---|---:|
| Players with neither a name nor a birth date | 145 |
| Players with no nation | 698 |
| Players whose every current contract lacks an end date | 3,209 |
| Players whose every current contract has wage 0 | 3,533 |
| Current contracts with wage 0 (all persons) | 9,718 |
| Staff with no current contract | 17,604 |
| Clubs with no nation | 42 |
| Clubs with no reputation | 34 |

The name and birth-date gaps are the same 145 rows: each lacks both a name and a birth date.
144 of them are unattached and 1 is contracted, which is why the free-agent pool reports 144
“no birth date” rejections. Consequences: the adapter must invent a contract end for 3,209
players (configured to `2027-06-30`), cannot price 3,533 players from the save, and must fall
back for clubs and persons without a nation.

### 3.4 Identifier integrity

| Fact | Count |
|---|---:|
| FM UIDs shared by more than one person | 165 |
| Persons involved in a shared FM UID | 342 |
| FM UIDs shared by multiple *players* | 26 |
| Duplicate club FM UIDs | 0 |
| Duplicate competition FM UIDs | 0 |

Internal `id` values are unique. The adapter keys OFM ids on internal ids (`fm-p-<person.id>`),
so shared FM UIDs survive as distinct players. The `person.fm_uid_status` column records
`partagé` for those rows.

---

## 4. How `world.db` is produced (Python importer)

Location: `~/apfootballmanager/importer/` (outside this repository).

| Entry point | Role |
|---|---|
| `run.sh` | Single command: `./importer/run.sh [save.fm]`; runs `python -I __main__.py` in the local venv |
| `__main__.py` | CLI: parse arguments, run extraction + checks + write |
| `pipeline.py::extract` | Full extraction into an in-memory `World` |
| `save.py::Save` | FMF container reader (`game_db`, `game_info`, `person_db_changes`, unnamed frames) |
| `players.py::read_players` | Player rows, 52 attributes, 15 position notes |
| `contracts.py::find_links` / `main_contract` | Contract chains; current vs historical |
| `competitions.py` | Competitions, nations, club leagues, membership |
| `clubs.py`, `people.py`, `staff.py`, `wages.py`, `values.py`, `finances.py`, `youth.py` | Supporting extraction and derivation |
| `database.py::write_world` | Applies `schema.sql` and writes `world.db` |
| `checks.py::run_checks` | Post-import integrity checks |

Extraction order (from the `pipeline.py` docstring): container and hash → name tables and persons
→ change journal → players → clubs/teams → contracts → staff → competitions and calendar → each
club's league → recalculated wages → estimated values → youth teams and coaches → finances →
colours.

Important properties for D7:

* The importer is **read-only** with respect to the `.fm` save; it writes a new `world.db`.
* Derived values (wages, market values, finances, youth) are computed by the importer, not read
  verbatim from FM. Any expansion inherits those derivations.
* `world.db` is **not** in this repository and is git-ignored (§8).

---

## 5. What the current world actually uses

The installed world at
`~/Library/Application Support/com.apfootballmanager.app/databases/apfm-fm24-2026.json`
(21,290,481 bytes, 2026-10-28, SHA-256
`7cec6bb41135c631c29fcfda9cec91957645984b191e1ec75bf8bd97a73f1d7d`) is the reference artifact.
Its install marker (`fm24-install.json`) reads `revision: 4`, while the current adapter declares
`WORLD_REVISION = 5` (`src-tauri/crates/fm24-adapter/src/install.rs:18`): the counts below are
identical in both, but a revision-5 rebuild is what the app will install next.

| Fact | Value |
|---|---:|
| Teams (clubs) | **240** |
| Players | **13,157** |
| — imported with `fm-p-*` ids | 13,045 |
| — generated by OFM | 112 |
| Staff | 4,257 |
| — imported with `fm-s-*` ids | 4,116 |
| — generated by OFM | 141 |
| Managers (real head coaches) | 219 |
| League definitions | 12 |
| National cups | 6 |

The `output/m7world/ADAPTER_REPORT.md` reference build (revision 5 of the adapter code) records the
same 240 / 13,157 / 4,257 totals and the following composition of the 10,045 imported contracted
players:

| Case | Players |
|---|---:|
| Own club | 8,063 |
| Loan in (owner outside perimeter) | 81 |
| Loan out (plays outside) | 694 |
| Internal loan | 557 |
| B team (owner's academy) | 464 |
| B team (plays there) | 186 |

Of the 20,448 unattached players, 10,480 are *eligible* for the free-agent pool and 3,000 are
chosen (seeded draw); the remaining 7,480 stay in `world.db`. Rejection reasons: 7,908 below the
required level, 1,769 outside the age range, 147 unknown nation, 144 no birth date, 0 no usable
position.

So the shipped world is **~16 % of the source players** (13,157 / 81,762) and **0.55 % of the
source clubs** (240 / 44,032).

---

## 6. Current perimeter rule

`src-tauri/crates/fm24-adapter/src/source.rs::perimeter` selects:

```sql
SELECT DISTINCT c.id, t.club_id
FROM competition c
JOIN competition_season cs        ON cs.competition_id = c.id AND cs.season = ?1
JOIN competition_season_team cst  ON cst.competition_season_id = cs.id
JOIN team t                       ON t.id = cst.team_id
WHERE c.kind = 'league' AND (c.playable = 1 OR c.simulated = 1)
```

With `start_season = 2026` this yields exactly the 12 leagues and 240 clubs above. The adapter
then hard-codes the expected shape in `pipeline.rs`:

```rust
pub const EXPECTED_CLUBS: usize = 240;
pub const EXPECTED_COMPETITIONS: usize = 12;
pub const EXPECTED_CUPS: usize = 6;
```

`checks.rs` fails the import when a run does not match those constants, and `pipeline.rs::run`
rejects a club that appears in two perimeter competitions. Any widening of the perimeter must
change these constants and the reference data together, or the adapter refuses to write.

---

## 7. Rust adapter: source → OFM mapping

Location: `src-tauri/crates/fm24-adapter/`. Entry point `src/main.rs` (`apfm-import
<world.db> <out-dir>`), orchestration in `src/pipeline.rs::run` (line 122).

### 7.1 Reads

| Function | File / line | What it reads |
|---|---|---|
| `Source::open` / `schema_facts` | `source.rs` | Opens `world.db` read-only; verifies hash and schema facts |
| `perimeter` | `source.rs:272` | 12 leagues / 240 clubs (SQL above) |
| `competitions` | `source.rs` | Competition rows + `competition_target` level/key |
| `clubs` | `source.rs` | Club name, nation, reputation, colours, finance, facilities |
| `players` | `source.rs:391` | Current players of the 240 clubs, 52 attributes, 15 positions |
| `unattached_players` | `source.rs:508` | Every player with no current contract (free-agent candidates) |
| `staff` | `source.rs:610` | Staff of the 240 clubs + role codes |

### 7.2 Conversions

| Concern | File | Rule |
|---|---|---|
| Attributes | `attributes.rs` | 52 FM attributes → 19 OFM attributes; `potential()` at line 164 |
| Positions | `positions.rs` | FM position codes → OFM positions; sweeper maps to CB |
| Placement | `placement.rs::place` (213) | Loans, affiliate/B teams, academy roles |
| Competitions | `competitions.rs::build` (33) | 12 leagues + 6 cups; continental cups deliberately excluded |
| Staff roles | `staff.rs::map_role` (40) | Head coach → manager; assistant/coach/scout/physio only; all other FM roles excluded |
| Free agents | `free_agents.rs` | Eligible → seeded draw → priced from same-level contracted players |
| Money | `config/adapter.toml` | Fixed £→€ `1.17`; never read online |

### 7.3 Config that changes the world

`src-tauri/crates/fm24-adapter/config/adapter.toml` is the single source of tunables. Values that
matter for any expansion:

* `base_year = 2026`, `start.date = 2026-06-01` (configured, not extracted — the save's date is
  ambiguous).
* `potential.ovr_points_per_ca = 0.31` (calibrated slope).
* `contracts.missing_end = 2027-06-30`.
* `loans.enabled = true`, `end_date = 2027-06-30`, `wage_contribution_pct = 0`.
* `free_agents` pool: 3,000 chosen from 10,480 eligible; age/level/nation filters in code.
* `squad.trim_generated_to_floor = true`, `calibrate_generated_to_division = true`,
  `promote_academy_before_generating = true`.
* `finance.restore_*` flags; non-wage net income paid as a permanent sponsorship.
* `positions.alternate_min_rating = 15`, `natural_within = 1`, `both_feet_min = 75`.

### 7.4 What is deliberately not mapped

* Continental competitions (no verified European rules in the reference data).
* 1,477 `other` competitions and the 86 2026 leagues with no `competition_target` row.
* Contract clauses: 190,321 rows in source, **none** in OFM. Only wage, start, end, club, team
  and identity cross the boundary.
* Most staff roles: OFM models assistant manager, coach, scout and physio (plus manager for head
  coaches). Directors, analysts, owners and other FM roles stay in `world.db` (journal code
  `staff.role_not_imported`, 14 occurrences in the reference run).
* Squad status, contract type, personality (beyond what the adapter synthesises) and FM's own
  competition history.

---

## 8. Licensing and distribution

FM data is proprietary to Sports Interactive / SEGA. The repository already treats it as
local-only:

* `NOTICE-APFM.md`: FM data “n'est ni fournie ni distribuée : elles sont lues dans la sauvegarde
  de l'utilisateur et restent sur sa machine.”
* `.gitignore` lines 61–64 ignore `world.db`, `apfm-fm24-*.json`, `apfm-fm24-*.extension.jsonl`
  and `fm24-import/`.
* `src-tauri/tests/distribution_guard.rs` fails the build if bundle resources mention `world.db`,
  `fm24`, `databases` or `apfm-`, if the ignore rules are removed, or if an FM24 world is
  committed under `databases/`.

Any D7 deliverable must preserve these invariants: no source database, no built world and no
derived player dump in git or in a distributed bundle. This document records counts and
mappings, not records.

---

## 9. Reproduction (read-only)

All figures above can be re-derived without writing to the source:

```bash
# Format and hashes
file ~/football-manager/*.fm ~/football-manager/*.zip
shasum -a 256 ~/football-manager/*.fm ~/football-manager/*.zip \
  ~/apfootballmanager/data/world.db
unzip -l ~/football-manager/'CUSTOM GRAPHICS- FM24 UPDATE 2026-27.zip' | tail

# Volume
sqlite3 ~/apfootballmanager/data/world.db \
  "SELECT count(*) FROM player; SELECT count(*) FROM club; SELECT kind, count(*) FROM competition GROUP BY kind;"

# Import provenance
sqlite3 -header -column ~/apfootballmanager/data/world.db "SELECT * FROM import_run;"

# Player partition by contract club (perimeter = 240 clubs)
sqlite3 -header -column ~/apfootballmanager/data/world.db "
WITH per AS (
  SELECT DISTINCT t.club_id cid FROM competition c
  JOIN competition_season cs ON cs.competition_id=c.id AND cs.season=2026
  JOIN competition_season_team cst ON cst.competition_season_id=cs.id
  JOIN team t ON t.id=cst.team_id
  WHERE c.kind='league' AND (c.playable=1 OR c.simulated=1))
SELECT
  (SELECT count(*) FROM per) AS perimeter_clubs,
  (SELECT count(DISTINCT p.person_id) FROM player p
     JOIN contract c ON c.person_id=p.person_id AND c.is_current=1
     JOIN per ON per.cid=c.club_id) AS contracted_perimeter,
  (SELECT count(*) FROM player p
     WHERE NOT EXISTS (SELECT 1 FROM contract c
                       WHERE c.person_id=p.person_id AND c.is_current=1)) AS no_contract;"
```

SQLite is opened read-only by these commands; none writes to `world.db`.

---

## 10. Non-claims

* This audit does **not** prove the save is an official or complete 2026/27 database. It proves
  what the file contains.
* The 81,762 players are **not** all usable: 20,448 have no current contract, 3,209 lack contract
  ends, 3,533 have zero wages in every current contract and 145 lack a name or birth date.
* The 44,032 clubs are **not** all playable: 42,848 have no 2026 league membership and 42 have no
  nation.
* The 99 leagues are **not** all modelled: 86 have no target row and only 12 are playable or
  simulated.
* The current 240-club / 12-league world is the **only** shape the adapter has been validated
  against; the hard-coded constants in §6 make anything else a deliberate code change, not a
  configuration switch.
