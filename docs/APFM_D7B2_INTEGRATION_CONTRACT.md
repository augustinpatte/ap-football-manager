# APFM D7B2 — Integration contract

**Status:** implemented and locally validated. **Date:** 2026-11-11. **Branch:**
`deepseek/d7-database-audit`.
**Audience:** anyone building on the external market — D7B3, the P5 transfer-negotiation work,
MCP/simulation tools, or a future screen.
**Companions:** [`APFM_D7B2_EXTERNAL_WORLD_INTELLIGENCE.md`](APFM_D7B2_EXTERNAL_WORLD_INTELLIGENCE.md)
(rules) and [`APFM_D7B2_LONG_CAREER_VALIDATION.md`](APFM_D7B2_LONG_CAREER_VALIDATION.md)
(measurements).

This document is the contract D7B3 must preserve. It covers the per-career cache, its invalidation,
the command flow, the IPC fields, error keys, persistence guarantees and the known overlap with P5.

---

## 1. Files on disk

```
<app data>/fm24-import/
├── registry.db                                  shared source snapshot (D7B1, read-only)
├── registry.db.staging                          only while a registry build runs
└── career-<seed>-<start>-<sha12>.db             one per career (D7B2, derived)
```

| Item | Rule |
|---|---|
| Cache directory | `<app data>/fm24-import/`, resolved by `commands::registry::registry_file` / `career_registry_path` |
| File name | `career-<career seed>-<start season>-<first 12 hex of source SHA-256>.db` |
| Ownership | Derived. Never authoritative. Safe to delete. |
| Shared snapshot | Opened `SQLITE_OPEN_READ_ONLY` + `PRAGMA query_only = ON`; never written by D7B2 |
| Save file | **Not touched.** No `Game` field, no SQL migration, no `databases/` entry |

The source hash in the name is deliberate: a rebuilt registry (new source file, new import) must
never be projected from an old career's rows.

### 1.1 Cache metadata (`meta` table)

| Key | Meaning |
|---|---|
| `schema_version` | Registry schema version the cache was built from; a mismatch forces a rebuild |
| `source_sha256` | Hash of the source `world.db`; must match the shared registry |
| `career_seed` | `Game::seed` at build time; must match the requesting career |
| `career_start_season` | Season the career's clock started in |
| `career_projected_season` | Absolute season the stored rows represent |
| `career_rows` | Row count of the last write, for diagnostics |

`career_projected_season` is the only state that "advances". Everything else is an identity check.

### 1.2 Invalidation rules (the whole rule)

`ensure_career_cache(registry_path, cache_dir, seed, today)`:

| Condition | Action |
|---|---|
| Cache missing, unreadable, wrong schema, wrong seed, wrong start season or wrong source hash | **Rebuild** from the shared snapshot |
| `cached_season == projection_season(today)` | **Reuse** (`rebuilt = false`, `seasons_advanced = 0`) |
| `cached_season < target` | Copy to staging, **advance one absolute season at a time**, VACUUM, rename |
| `cached_season > target` | **Rebuild** (an older save loaded after a newer one; retirement cannot reverse) |

No other cache is consulted and no daily job exists: the cache is refreshed lazily by the registry
commands, at most once per season of career clock.

### 1.3 Build safety and concurrency

* The build writes to `<cache>.staging`, runs `VACUUM`, then `rename`s over the live file. An
  interrupted build leaves the previous cache intact.
* A process-wide mutex (`career::CACHE_MAINTENANCE`) serialises cache maintenance, because every
  build uses the same staging name. Two commands cannot corrupt each other's half-built file.
* Two **processes** are not coordinated. If they race, the last complete rename wins; each result
  is a valid cache for its own season and the next call fixes it. Correctness is preserved; only
  work is duplicated.
* The mutex is held only inside `ensure_career_cache`, which runs on a blocking worker. It is never
  held across an `await`.

## 2. Command flow

All external-market reads follow the same path:

```
Tauri command (async, main runtime)
  ├─ registry_file(app)                     → shared registry.db path
  ├─ context_for(state)                     → today, seed, excluded_ids from the live Game
  └─ tauri::async_runtime::spawn_blocking   → blocking worker
       ├─ career::ensure_career_cache(...)  → cache path at today's season
       ├─ registry::open_registry(path)     → READ_ONLY
       └─ registry::query_registry / get_registry_player
```

| Command | Behaviour |
|---|---|
| `search_external_players` | Ensures the cache, then runs the unchanged D7B1 paginated query |
| `get_external_player` | Ensures the cache, then reads one detail row |
| `preview_external_recruitment` | Loads detail from the cache, previews against the live `Game` |
| `recruit_external_player` | Loads detail, previews, mutates through `mutate_active_game` |
| `get_external_registry_status` | Reads the **shared snapshot only** (coverage counts, bytes) |

`RegistryContext` (internal to `commands/registry.rs`) now carries:

```rust
pub struct RegistryContext {
    pub today: NaiveDate,      // from Game.clock, never the host clock
    pub seed: u64,             // from Game.seed
    pub excluded_ids: Vec<String>, // roster + retired archive, fm-p-* and fm-n-*
}
```

Nothing on the wire changed for `RegistryContext`; it is not serialised.

**Coverage status is not the career market.** `get_external_registry_status` reports the shared
snapshot's static counts (81,762 stored, 69,301 eligible, 9,843 active-world, etc.). It does not
report the projected market's population, which falls over a long career. A screen that wants
"players available today" must query search, not status.

## 3. IPC contract

### 3.1 Unchanged request/response shapes

`RegistrySearchRequest`, `RegistryPage`, `RegistrySummary`, `RegistryPlayerDetail`,
`ExternalRecruitOutcome` and `RecruitExternalResponse` keep their D7B1 shape. A frontend built for
D7B1 keeps working.

### 3.2 `ExternalRecruitPreview` additions

```ts
export interface ExternalRecruitPreview {
  playerId: string;
  freeAgent: boolean;
  alreadyInWorld: boolean;
  fee: number;
  /** New in D7B2: the contracted club's own floor. `fee` is never below it. */
  minimumFee: number;
  /** New in D7B2: the deterministic club decision for this offer. */
  clubDecision: "accepted" | "countered" | "rejected";
  weeklyWage: number;
  contractYears: number;
  contractEnd: string;
  canAffordFee: boolean;
  wagePolicyAllows: boolean;
  windowOpen: boolean;
  blocker: string | null;
}
```

* The one-click command offers `max(askingPrice, minimumFee)`, so `clubDecision` is `accepted` for
  the default path and `fee == max(askingPrice, minimumFee)`.
* `minimumFee` is `0` for a free agent.
* `blocker` is a translation key, never prose. A stale preview cannot bypass a changed decision:
  `recruit_external_player` recomputes the whole preview before mutating.

### 3.3 Rust-side offer API (for P5 / D7B3, not yet on the IPC surface)

```rust
// ofm_core::external_registry
pub fn preview_external_recruitment_with_offer(
    game: &Game,
    external: &ExternalPlayer,
    offer: Option<u64>,
) -> Result<ExternalRecruitPreview, String>;

pub fn recruit_external_player_at_offer(
    game: &mut Game,
    external: &ExternalPlayer,
    offer: Option<u64>,
) -> Result<ExternalRecruitOutcome, String>;

// ofm_core::external_world
pub struct ExternalOfferContext { /* seed, season, prices, abilities, reputations, player_id */ }
impl ExternalOfferContext {
    pub fn minimum_fee(&self) -> u64;
    pub fn decide(&self, offer: u64) -> ExternalOfferDecision;
}
pub enum ExternalOfferDecision {
    Accepted { fee: u64 },
    Countered { fee: u64 },
    Rejected { minimum: u64 },
}
```

`None` reproduces the one-click request. `Some(offer)` is the explicit-offer path: rejected below
75% of the floor, countered at the floor, accepted at or above it. The rule is pure: the same
context and offer always return the same decision, including the seeded valuation jitter.

**Do not re-implement club valuation in P5 or D7B3.** Reuse `ExternalOfferContext`.

## 4. Error keys

| Key | Meaning |
|---|---|
| `be.error.registry.offerRejected` | **New in D7B2.** The selling club refused the offer as too low. Nothing mutated. |
| `be.error.registry.playerAlreadyInWorld` | Identity already on a roster or in the retired archive |
| `be.error.registry.playerNotAvailable` | Outside availability / age projection |
| `be.error.registry.playerIncomplete` | Stored record lacks a name, birth date, nation, position or attributes |
| `be.error.fm24.registryMissing` | Shared registry not built or not readable |
| `be.error.fm24.registryBuildFailed` | Cache/registry write failed |
| `be.error.fm24.registryQueryFailed` | Query failed |
| `be.error.fm24.writeFailed` | App-data path could not be resolved |

`be.error.registry.offerRejected` exists in all 12 locales (`src/i18n/locales/*.json`).

## 5. Persistence and save compatibility

| Concern | Guarantee |
|---|---|
| Save format | **Unchanged.** No new `Game` field, no new table, no migration, no `MIGRATION_COUNT` bump |
| Seed | `Game.seed` already persisted in `game_meta.seed`; existing round-trip test |
| Promoted player | Normal row in the existing `players` table (D7B1 path, unchanged) |
| Cache | Outside the save; deleting it rebuilds the same projection |
| Save divergence | Two saves with different seeds get different cache files; two saves with the same seed and different clocks rebuild but always show the correct season |
| Old saves | Load unchanged; the external market is projected for their clock on first search |

No claim is made that save size or save/load time were re-measured in D7B2; the absence of a
format change is the guarantee.

## 6. P5 compatibility

P5 and D7B2 overlap only where "a club decides whether to sell". Everything else is disjoint:
D7B2 touches no match engine, no M12, no 2D viewer, no AI formation/appointment/squad selection, no
active-club strategic AI, no match-day logic, no competition pyramid, no simulation scope and no
frontend screen.

| Surface | Change | P5 risk |
|---|---|---|
| `domain` / `Game` / save schema | none | **None** |
| `ExternalClub` (ofm_core) | **new required field `reputation: i64`** | **Compile-time break** wherever P5 constructs an `ExternalClub` literally; add the field |
| `ExternalRecruitPreview` | **new required fields `minimum_fee`, `club_decision`** | **Compile-time break** for literal construction / exhaustive patterns; frontend type updated |
| `ExternalOfferContext`, `ExternalOfferDecision` | new public types | **None** unless P5 also invents club negotiation — then **duplication risk** |
| `preview_external_recruitment_with_offer`, `recruit_external_player_at_offer` | new public functions | **None**; additive |
| `ofm_core::external_world` | new module | **None**; no existing symbol renamed |
| `fm24_adapter::career` | new module | **None** |
| `fm24_adapter::registry::{SCHEMA, group_name, footedness_name}` | `pub(crate)` (was private) | **None** outside the crate |
| `RegistryContext` | new `seed` field | **None**; internal to `commands/registry.rs` |
| Commands | same names/signatures | **None** on the wire |
| Locales | `be.error.registry.offerRejected` added | **None** |

**Merge guidance.** If P5 changes `ofm_core::external_registry` or `external_world`, keep D7B2's
public names and add parameters/options rather than renaming. If P5 implements active-world
negotiation, it should translate its own state into `ExternalOfferContext` at the boundary instead
of copying the valuation formula. If P5 changes the registry schema, bump
`REGISTRY_SCHEMA_VERSION`; D7B2 caches rebuild automatically.

**Residual risk:** MEDIUM for literal-construction breaks and duplication; LOW for data model,
save format and IPC. No attempt was made to read P5's worktree, so this is a surface analysis, not
a diff against P5.

## 7. Operational notes

* **First search after install**: builds the cache. Real source, season 1: ≈ 2.2 s; season 30 from
  scratch: ≈ 20 s, worst case. One advance per later season: sub-second to a few seconds.
* **Query**: 13–50 ms for a 25-row page over the real source, depending on season.
* **Disk**: 58.6 MB at season 1, 21.6 MB at season 20+, per career.
* **Memory**: the five-cache benchmark peaked between 1.6 and 2.5 GiB across runs in one process; a
  single command is far lower. No production memory target is claimed.
* **Failure**: a cache failure returns a translation key and leaves the previous cache usable; the
  shared registry is never modified.
* **Cleanup**: delete `career-*.db` to reclaim disk. Nothing else depends on them.

## 8. What D7B3 may and may not assume

May assume:

* the cache is correct for the career's clock, or `ensure_career_cache` fixes it;
* every cache row is a pure function of source row + seed + season;
* retirement is monotone and stable across reloads and rebuilds;
* `ExternalOfferContext` is the only club-valuation rule;
* no external state is in the save.

May **not** assume:

* the external market is a fully simulated world (no squads, fixtures, finances or bidding AI);
* the market keeps 69k players forever (it stabilises near 14k on the real source);
* elite players are regenerated (the generated ceilings do not replace OVR 85+);
* an explicit offer screen exists on the IPC surface (the rule exists; the wire does not);
* the coverage status reflects the career market (it reflects the static snapshot).
