# APFM D7B1 — A safe global player registry and recruitment pathway

**Status:** implemented and locally validated. **Date:** 2026-11-11. **Branch:**
`deepseek/d7-database-audit`.
**Scope:** make the eligible players of the player's own FM24 source searchable and signable
without simulating 81,762 people. The 240-club / 13,157-player active world, M12, the 2D viewer,
the match engine, competition pyramids and the default simulation scope are unchanged.

Companions:
[`APFM_D7A_SOURCE_DISCOVERY.md`](APFM_D7A_SOURCE_DISCOVERY.md),
[`APFM_D7A_IMPORT_READINESS.md`](APFM_D7A_IMPORT_READINESS.md),
[`APFM_D7B1_PERFORMANCE_AND_VALIDATION.md`](APFM_D7B1_PERFORMANCE_AND_VALIDATION.md).

The D7A decision stands: **NO-GO** for simulating the whole source database, **GO** for staged,
opt-in expansion. D7B1 is the first staged step: a worldwide *market* around the active world, not
a bigger simulated world.

---

## 1. The problem D7B1 solves

`world.db` holds 81,762 players in 44,032 clubs. The shipped world imports 13,157 of them into 240
clubs because those are the clubs whose competitions the game simulates every day. Importing the
rest into `Game.players` would put them on the daily turn loop, the AI market, squad lists, wage
bills and season rollover — the cost D7A measured as ~10^5× and rejected.

Players outside the active world are still interesting to a manager: a free agent in Brazil, a
wonderkid at an external club, a veteran whose contract has run out. D7B1 makes those people
findable and signable while keeping them out of the simulation until the moment one is signed.

## 2. Architecture comparison and choice

| Option | What it means | Why not / why |
|---|---|---|
| **A — all players in `Game`** | Import every source person as a normal `Player` | Daily simulation, AI market, persistence and memory scale with 81,762; exactly the D7A NO-GO. |
| **B — separate external registry only** | Search a sidecar; no way to sign anyone | Cheap, but fails the requirement: no recruitment, no promotion, no save/load proof. |
| **C — hybrid: lightweight registry, promotion on recruitment** | Indexed sidecar of source records; a signed player becomes one normal `Game` player | **Chosen.** The active world never grows until the manager acts; identity, contracts and persistence reuse the existing player pipeline. |

Option C is implemented as:

```
world.db (read-only, player's own file)
   │  one install-time build
   ▼
<app data>/fm24-import/registry.db      ← sidecar, indexed, queried read-only
   │  search / detail / preview
   ▼
Tauri commands ──► frontend service (one page at a time)
   │  recruit
   ▼
Game.players  ← one promoted Player, existing player table, no migration
```

The registry is **not** a second game. It has no teams, competitions, fixtures, wages paid or
daily turn. External clubs are records (`fm-c-<club id>`, name) attached to a player's contract;
they never become `Team` objects.

## 3. Identity

**Authoritative identity is the source internal `person.id`**, projected as `fm-p-<person_id>`.
FM Unique IDs are stored as metadata only because the source has 165 shared UIDs; using them as
identity would merge different people.

The registry stores one row per source player (`registry_player.person_id PRIMARY KEY`,
`ofm_id UNIQUE`). No source player is dropped:

| Status | Meaning | Real-source count |
|---|---|---:|
| `eligible` | Usable external player, in the source's 14–40 build-time age band | 69,301 |
| `active_world` | Club is inside the active simulation perimeter | 9,843 |
| `missing_identity` | No usable name or birth date (**never fabricated**) | 145 |
| `missing_nation` | Source nation exists but OFM has no mapped code (mail fail, never guessed) | 1,880 |
| `no_position` | No usable FM position note | 0 |
| `outside_age_range` | Older than 40 or younger than 14 at build time | 593 |

These are the counts of the actual source on this machine, measured by
`build_registry`; the performance/validation document records the run. `missing_nation` is larger
than D7A's 698 null-nation rows because it also counts source nation names OFM's catalogue cannot
map; those players are searchable only when `includeIncomplete` is requested, and cannot be
signed.

Every build decision is written to `registry_journal` (one row per player plus conversion notes),
so a coverage screen can explain each exclusion. Nothing is silently discarded.

### 3.1 Source mappings preserved

Each registry row keeps:

* the source first/last/common name and a display name (common name when present);
* birth date, source nation name and the mapped OFM nation code;
* the converted `Position`, alternate positions, footedness, weak foot, and the full converted
  `PlayerAttributes` JSON;
* OVR and potential, the source market value and its provenance (`save` / `générée` /
  `estimated`), the weekly wage and its provenance;
* the owning club id, name, nation and reputation;
* the current contract end and the ownership projection.

Attributes and position mappings are **frozen at the source snapshot**. This is deliberate:
re-deriving them from the live game would need the source engine's CA/PA model inside a query.
The limitation is named in §9.

## 4. Build and lifecycle

`fm24_adapter::registry::build_registry(world_db, destination)`:

1. opens the source read-only through the existing `Source` (no write, no mutation of `world.db`);
2. reads all players with `Source::every_player()` (current contract chosen deterministically);
3. classifies and converts each row with the same `convert_player` the world import uses;
4. writes a staging file (`registry.db.part`) in one transaction, sets `PRAGMA user_version`, then
   atomically renames it into place. A failed build leaves the previous registry usable;
5. records `meta` (schema version, source SHA-256, source game/importer version, start season,
   build date, and every coverage count).

Installation wiring: `install_fm24_world` installs the world first, then builds the registry into
`<app data>/fm24-import/registry.db`. A registry failure does not lose a playable world: the
command returns `registryError` and `get_external_registry_status` keeps reporting search as
unavailable. The registry is never written into `databases/`, which the world listing scans.

Query time opens the file with `SQLITE_OPEN_READ_ONLY` and `PRAGMA query_only = ON`. A missing file
is a translation key (`be.error.fm24.registryMissing`), not a silently created empty database.

Nothing FM-derived is committed: the source is read on the player's machine and the sidecar is
written under the app data directory. Distribution tests and `distribution_guard.rs` are untouched.

## 5. Search contract

One request (`RegistrySearchRequest`) carries only screen filters:

* text — accent-folded substring over player name **and** club name;
* nation (OFM code), position group (GK / DEF / MID / FWD), min/max age, club;
* availability (`all` / `free_agent` / `contracted`), projected against the career's clock;
* sort (`name` / `age` / `ovr` / `value` / `club`), direction, page and page size.

The backend adds, and the screen cannot override:

* `today` from `game.clock` (so age and contract projections follow the career, not the host clock);
* the exclusion set: every `fm-p-*` id already on a roster or in the retired archive;
* the page-size clamp (1–100, default 25).

The response is one `RegistryPage`: 25 projected summaries by default, `total`, page metadata and
counts. The full registry is never loaded into the frontend. Each summary has only the columns a
row needs: id, name, age, nationality, position, OVR/potential, club, ownership, contract end,
market value, weekly wage, asking price and status.

The exclusion set is the display half of the duplicate guard. It is deliberately derived from the
live game, not from a second copy of the roster in the sidecar, because the same source player can
be in a different state in a different save.

## 6. Recruitment

All rules live in `ofm_core::external_registry`; the adapter and the Tauri command only transport
data. `preview_external_recruitment` returns the fee, wage, contract length and the single blocker;
`recruit_external_player` recomputes the same preview and refuses on the same terms, so a stale
screen cannot slip a different deal through.

### 6.1 Free agents vs contracted players

* **No club, or a contract that ended on or before today** → free agent. He signs on the club's
  standard terms (`standard_contract_terms`), through `sign_free_agent`, with the same wage policy
  and the same message a normal free signing produces. No fee; the transfer window does not block
  him.
* **Still under contract at an external club** → bought at a deterministic asking price. The window
  must be open, the fee must fit cash **and** transfer budget (`can_afford_fee` / `fee_block`), and
  the wage policy must allow the contract. External clubs do not run an AI in D7B1: they accept
  exactly the asking price.

### 6.2 Asking price

```
asking_price(value, contract_end, today):
  free agent / expired / value = 0        → 0
  ≤ 183 days left                          → value × 1.00
  ≤ 365 days left                          → value × 1.20
  ≤ 730 days left                          → value × 1.50
  > 730 days or no end recorded            → value × 1.75
  rounded down to the nearest 1,000, minimum 1,000 when value > 0
```

An unknown contract end deliberately takes the highest multiplier: the source cannot prove the
contract is short. The price is recomputed from the stored value and end date on every read, so it
does not drift.

### 6.3 What a promotion mutates

Validation happens before the first mutation, because `StateManager::update_game` cannot roll a
closure back:

1. the managed club exists;
2. the source identity is not already on a roster or in the retired archive;
3. the record has a name, birth date, mapped nation, position and attributes;
4. the age is within 14–37 (the registry's availability projection);
5. the window is open (contracted only);
6. the fee fits cash and budget;
7. the wage policy allows the contract.

Then, and only then:

* the promoted `Player` is built from the source data (`Player::new` plus preserved natural /
  alternate positions, foot, weak foot, potential, value) and `refresh_player_derived` recomputes
  what the game derives;
* a free agent goes through `sign_free_agent`;
* a contracted player posts one `TransferFeeOut` cash entry, takes the club's jersey number, gets a
  `PermanentTransfer` movement entry naming the external club and the fee, decrements the transfer
  budget, and is filed in one competition's transfer log through `log_completed_transfer`;
* the response carries the mutated `Game` exactly like the other command families, plus the
  outcome (fee, wage, contract end, from-club).

There is **no second owner and no second contract**: the promoted player is a single `Game` player
with one contract ledger, and the source row is excluded from later searches by the live roster.

### 6.4 Save / reload

No new persisted field, no SQL migration. The promoted player is a normal row in the existing
`players` table (attributes, movement history, contract and stats JSON included). A round-trip
test writes a career after a promotion, reads it back and asserts: exactly one player under
`fm-p-77`, the right club, contract club, natural/alternate positions and free-agent movement
entry; promoting the same source record again returns `be.error.registry.playerAlreadyInWorld`.

## 7. Deterministic external-world progression

D7B1 does **not** run a daily simulation of the registry. The rules that change over time are
projections evaluated on read, once per row, against the career's date:

* **Availability / retirement:** a player older than 37 is no longer returned by searches and is
  refused by recruitment (`MAX_EXTERNAL_AGE`). The rule matches on both sides.
* **Contracts:** a contract end on or before `today` makes the player a free agent in search,
  detail, preview and recruitment. No job runs at midnight.
* **Age:** computed from the birth date and `today`; it advances with the career clock.
* **Attributes / potential / value:** frozen at the source snapshot. There is no external
  development or decline in D7B1.

This is deterministic (same source and date produce the same answer), bounded (O(1) per row in a
query, no per-day work), and honest about what is not simulated.

## 8. Delivered code

| Area | Files |
|---|---|
| Promotion rules, asking price, preview/recruit, 7 tests | `src-tauri/crates/ofm_core/src/external_registry.rs` |
| Sidecar schema, builder, classification, journal, paginated query, detail, projection, 8 tests + benchmark | `src-tauri/crates/fm24-adapter/src/registry.rs` |
| All-player source query and club nation/reputation | `src-tauri/crates/fm24-adapter/src/source.rs` |
| Install wiring, registry path, coverage in install response | `src-tauri/src/commands/fm24_world.rs` |
| Search, detail, preview, recruit, status commands and 5 tests | `src-tauri/src/commands/registry.rs` |
| Command registration | `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` |
| Typed pagination/recruitment service and 4 tests | `src/services/registryService.ts`, `registryService.test.ts` |
| Error keys in all 12 locales | `src/i18n/locales/*.json` |
| Install response type | `src/services/worldDatabaseService.ts` |
| Save/reload test | `src-tauri/crates/db/src/game_persistence.rs` |

No screen is added in D7B1: the deliverable is the tested backend/IPC/service pathway. A screen
consumes `searchExternalPlayers` / `previewExternalRecruitment` / `recruitExternalPlayer` without
ever receiving more than one page. This is stated again in §10.

## 9. Known limitations

1. **Frozen snapshot.** Attributes, positions and potential do not evolve outside the world. If the
   source save changes, the registry must be rebuilt.
2. **No external-club AI.** External clubs accept the asking price and never bid, reject, negotiate
   or replace a sold player. Their squads are not simulated.
3. **No external competitions or finances.** External clubs are names and ids, not `Team` objects.
4. **The 14–40 build band and 38+ retirement projection are policy, not source facts.** They are
   named constants and can be tuned without touching stored identity.
5. **Some source records cannot be used:** 145 without a usable name/birth date, 1,880 whose nation
   OFM cannot map, 593 outside the age band at build time. They are classified, not dropped; the
   coverage screen can show them.
6. **26,144 of 81,762 values are estimated** (source had no value), 2,490 are the source's
   generated values and 53,128 are the source's own. `value_source` keeps that distinction; an
   asking price built on an estimate is still deterministic.
7. **No serialized frontend screen yet**, so accessibility/i18n review of a screen is future work;
   the error keys are translated now.
8. **A stale registry is not detected against a changed `world.db` beyond the recorded SHA-256.**
   Rebuilding is the supported correction.

## 10. What D7B1 does and does not claim

**Claimed, proven locally:**

* all 81,762 source players are stored and classified, with no silent drop;
* 69,301 are eligible external players, 19,540 of them free agents at the source snapshot;
* search filters by name, club, nation, position, age and availability with pagination;
* a promoted player is signable, pays the right fee, keeps his source identity and survives
  save/reload as one player;
* the active world's daily simulation, memory and persistence are unchanged.

**Not claimed:**

* that all 81,762 are recruitable — 12,461 are explicitly not (`active_world`, missing identity,
  unmapped nation, outside age), and 3,209 contracted players have no contract end so they take
  the 1.75× price;
* that external players develop, retire through a simulated event, or move between external clubs;
* that a UI screen exists;
* that any of this is distributed with FM data. The registry is built on the player's machine from
  the player's own `world.db`.

## 11. Reproduction

```bash
# Registry + promotion rules + install commands + save/reload
cargo test --manifest-path src-tauri/Cargo.toml --workspace

# The scaling benchmark (synthetic, no real data)
cargo test --manifest-path src-tauri/Cargo.toml -p fm24-adapter --lib large_registry \
  -- --ignored --nocapture

# Frontend service and every locale
npm test
npm run build
```

Real-source build and coverage (local only; nothing is written into the repository):

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p fm24-adapter --lib registry::
```

The measured real-source and scaling numbers are in
[`APFM_D7B1_PERFORMANCE_AND_VALIDATION.md`](APFM_D7B1_PERFORMANCE_AND_VALIDATION.md).
