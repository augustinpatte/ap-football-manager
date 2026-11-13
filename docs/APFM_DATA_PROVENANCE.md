# Player database provenance and quality (Run D, part B)

Base: `validation/runs1-4-m12`. Everything below was measured on the code in this branch; numbers
come from the seeded generated world (seed `20260820`, `generate_world_seeded`) unless stated. The
FM24 data (`world.db`) is the user's own local file and was **not available** in the Cloud session:
claims about it are read from the importer's code and tests, not from the data.

## 1. Where players come from

| Source | What it is | Real people? |
|---|---|---|
| **Random World** (the built-in default; `ofm_core::generator`) | 440 generated clubs, 23 players each (19 seniors + 4 academy) plus 1,760 free agents = 11,880 players. | **No. Entirely fictional.** |
| **`.ofm` packages / world JSON** (`src-tauri/databases/`, `docs/modding/`) | Whatever the author wrote. The repository ships only fictional examples (`classic-sixteen`: "Brussels Royal", …). | Only if the author made them so; the game cannot verify it. |
| **FM24 import** (`crates/fm24-adapter`, binary `apfm-import`) | Reads the user's own FM24 save read-only: 240 clubs in 12 divisions, players keyed `fm-p-<person.id>`. Clubs under the 15-senior floor are topped up with **generated** fillers (ids without `fm-p-`, all listed in the import report). | The people in the user's FM24 file. That file is a commercial game database which itself contains generated players ("regens"); the importer has no way to tell them apart. |

The repository contains no licensed football data and must not (`NOTICE-APFM.md`,
`src-tauri/tests/distribution_guard.rs`). Nothing in this run adds, scrapes or redistributes any.

### How the Random World makes a player

* **Club names** are `pattern + city`: `{} FC`, `{} United`, … over per-nation city pools
  (`generator/clubs.rs`). "Brussels FC" exists because the Belgian city pool contains "Brussels"
  and the plain `{} FC` pattern was drawn for it. It is not Royal Union Saint-Gilloise, Anderlecht
  or any real club.
* **Names**: 17 name pools (AR, BE, BR, DE, ENG, ES, FR, GB, HR, IE, IT, NIR, NL, PT, SCO, SE,
  WAL), each **20 first names × 20 surnames = 400 possible names**. A nationality without its own
  pool (most of ~210) borrows a pool of its confederation, else any pool. First and surname are
  drawn independently, so a first name is occasionally a famous one ("Romelu Goossens") and
  occasionally a real person's name can come out by chance; none is looked up from a real roster.
* **Ratings**: attributes are generated per position around a target overall; `ovr` is then
  computed from them (`player_rating::natural_ovr`). Across all 11,880 players stored `ovr` and the
  attribute-derived value differ by at most 0.5, `potential >= ovr` always, `ovr` runs 23–84
  (median 64). This was measured **before** the career-start identity upgrade; D6 found that the
  upgrade left `ovr` describing the old coarse position and fixed it, so the claim now holds in
  both states ([`APFM_D6_WORLD_DATA_AUDIT.md`](APFM_D6_WORLD_DATA_AUDIT.md) §5.1).
* **Club membership**: every generated player's `team_id` names a generated club; no orphan.

## 2. The Brussels FC case (reproducible)

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --test generated_world_integrity \
  brussels_fc_is_a_generated_club_with_a_full_generated_roster
```

Seed `20260820` yields "Brussels FC" (country BE) with 23 players (19 seniors, 4 academy). All are
generated: the test asserts they carry generator ids, not imported `fm-p-` ids, and that the roster
passes the audit. Before the fixes below, the same seed gave this club a *Simon Simon* and, among
others, a Welsh name (*Gruffydd Rees*) with the nationality EG. Names change with the generator; the
test pins the provenance, not the names. **The user's observation was right: those players are
fictional, by design.**
The game's world picker described this world as "a randomly generated league … across Europe", which
undersold it (the world spans every nation). The description now says every club, player and name is
invented.

## 3. Defects found and fixed

All have a test that was seen failing first (`tests/generated_world_integrity.rs`, unit tests in the
modules named).

| Defect | Evidence (seed 20260820) | Fix |
|---|---|---|
| **Two different players with the same name, birth date and nationality** (both academy youngsters: *Bruno Oliveira* BR 2009-08-01, *Luke Green* ENG 2009-05-04). On screen, in news and in search they are one person. | 2 clashes in 11,880 | `generator/unique_identity.rs`: the later player moves to the next free birth date. No random stream touched, ids and names unchanged, still deterministic per seed. |
| **First name equal to surname** ("Simon Simon", "Scott Scott", "Laurent Laurent"). | 14 players | `generation::draw_name` redraws only the surname, only on a clash. A world's names are unchanged up to its first clash; after it the extra draw shifts the random stream, so **a given seed now produces a different (still deterministic) world than before this change**. Saves are unaffected; only newly generated worlds differ. |
| **World picker quoted 9,680 players** (440 × 22). The generator builds 23 per club and 1,760 free agents. | real total 11,880 | `STANDARD_WORLD_TEAM_COUNT` / `STANDARD_WORLD_PLAYER_COUNT`, used by `list_world_databases`; a generator test fails if they drift. |
| **Invented careers were indistinguishable from real ones.** For every roster-baseline world (this includes the FM24 import and any `.ofm` package) `history_generation` writes, for each past season from the player's 18th birthday, a career line **at his current club** with invented appearances and goals. A 30-year-old with 12 generated seasons "spent his whole career" at the club he just joined. | by construction (`upsert_player_career`) | `CareerEntry.synthetic` (`serde(default)`; JSON column, so no migration). The profile marks such seasons "Simulated" and explains. |
| **A recorded career season was overwritten.** If a world file recorded a player's 2030 season at another club, history generation deleted it (`career.retain(season != …)`) and replaced it with the current club. | regression test failed before the fix | A season with a non-synthetic record is kept as is; awards read its real numbers; only the other seasons are generated. |

### Save compatibility

* `CareerEntry.synthetic` is `#[serde(default)]`. The career is a JSON column and the repository
  falls back to an **empty** career if the JSON does not parse, so a missing default would have
  silently erased careers: `test_career_written_before_the_synthetic_flag_still_loads` fails when the
  default is removed (verified), and passes with it.
* Player ids are untouched everywhere. The identity and name fixes run only when a world is
  *generated*; they never touch an existing save or an imported world.
* Careers already in old saves are **not** flagged (their generated and real seasons cannot be told
  apart after the fact); they read as before.
* No database file is written or overwritten by any of this.

## 4. Automated integrity checks

`ofm_core::world_integrity::audit_players(teams, players, limits)` is pure and read-only. Errors:
duplicate player id, duplicate identity (name + birth date + nationality), `ovr`/`potential` outside
1–99 or `potential < ovr`, attribute above 100, stored `ovr` more than 2.0 from the attribute-derived
value, unparsable birth date, unknown club, unknown loan parent, `team_id` ≠ loan club, retired
player still on a club, starting XI naming a stranger, fewer than 15 seniors. Warnings: implausible
age (outside 14–45), more than 45 players on a club's books.

Used by:

* `tests/generated_world_integrity.rs` — five seeds must audit with **zero errors**, zero identity
  clashes and no repeated-word names; same seed → same world;
* the FM24 report (`checks.rs`, `integrity.world_audit`) — **advisory** (never blocks): this is what
  will flag a PSG-sized roster or an identity clash on the real import;
* 14 unit tests that feed it each defect, including the D6 contract checks, plus the separate
  `audit_competitions` graph check ([`APFM_D6_WORLD_DATA_AUDIT.md`](APFM_D6_WORLD_DATA_AUDIT.md)
  §3).

## 5. Known problems not fixed

* **Name pools are tiny.** 400 names per nationality in a 11,880-player world: 2,298 (name,
  nationality) pairs are shared by two or more players. They are different people (different birth
  date or id), so the audit does not flag them, but a search for "Thomas Mertens" finds several.
  Fixing it means authoring larger name lists.
* **Name and nationality can disagree** for nationalities without a pool (*Gruffydd Rees*, EG;
  *Daan Mulder*, AE): a deliberate fallback documented in `fallback_pool`, arbitrary rather than
  wrong by construction.
* **"Reference History" label.** The world picker shows `worldSelect.historyMode.reference` for any
  imported world, but a roster-baseline world's past is still simulated (§3). Only the per-season
  "Simulated" mark is accurate; the label is not changed here.
* **Old saves** keep unflagged generated careers (§3).
* **Identity uniqueness is enforced at world generation only.** Youth intakes and scouted prospects
  created during a career are not checked against the existing players.
* **FM24 data was not inspected.** The following need the user's `world.db` and are *not* verified:
  * *PSG's oversized squad*: the cause is in `placement.rs` (contract club vs registered team; loans,
    reserve and youth sides — documented there with PSG's Pierre Gibaud as a test). Whether the
    current import is right for PSG needs a run; the new `integrity.world_audit` will warn above 45
    players.
  * *Missing expected players*: the import is a 240-club perimeter plus a 3,000-player free-agent
    pool out of 20,448; excluded players are journaled (`players.none_dropped_silently`). Whether a
    given player is missing is a question of that journal.
  * *Fictional identities inside FM24 itself* (regens) cannot be detected from the file.
* **No worldwide real-roster rebuild** was attempted, as instructed. Real rosters require licensed
  data the repository does not have.

## 6. Commands

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --test generated_world_integrity
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --lib world_integrity unique_identity history_generation
cargo test --manifest-path src-tauri/Cargo.toml -p db --lib career
cargo test --manifest-path src-tauri/Cargo.toml -p fm24-adapter
```

On the machine with the FM24 save, run the importer and read `integrity.world_audit` in its report;
treat `duplicate_identity`, `unknown_club` and `loan_club_mismatch` as bugs and `squad_too_large` as
a prompt to look at that club's placement.
