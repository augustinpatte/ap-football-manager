# APFM D7B2 — Long-career validation

**Status:** locally measured and validated. **Date:** 2026-11-11. **Branch:**
`deepseek/d7-database-audit`.
**Companion:** [`APFM_D7B2_EXTERNAL_WORLD_INTELLIGENCE.md`](APFM_D7B2_EXTERNAL_WORLD_INTELLIGENCE.md)
— the projection rules; [`APFM_D7B2_INTEGRATION_CONTRACT.md`](APFM_D7B2_INTEGRATION_CONTRACT.md) —
the cache and IPC contract.

All numbers are single runs on one machine, one source save and one build of this branch. They are
evidence that the design behaves as described; they are **not** CI thresholds and not guarantees
for every host. Nothing FM-derived is committed: the real benchmark needs the player's own file and
is `#[ignore]`d.

---

## 1. Environment and method

| Item | Value |
|---|---|
| Host | macOS, Apple M4, arm64 |
| Toolchain | rustc / cargo 1.95.0 |
| Rust test profile | `[profile.test] debug = 0`, `incremental = false` (optimised test binaries) |
| Real source | `~/apfootballmanager/data/world.db`, 241,463,296 bytes, SHA-256 `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` |
| Source save | FM 24.3.0, 2026/27 season, SHA-256 `70915fb59a6209e0b1a76789fe0ae912fac4aac9347a123b63350478ec018d4e` |
| Career seed for the real benchmark | 777 (fixed for comparability) |
| Season for a "year N" row | `2026 + N`, measured on 1 August |
| Active world | 240 clubs, 13,157 players |

### 1.1 Real-source cache benchmark

`fm24_adapter::career::tests::benchmark_real_career_cache` builds the real registry once, then a
**cold** career cache from scratch for seasons 1, 5, 10, 20 and 30, and times a full rebuild, a
first search and a repeat search:

```bash
APFM_D7B2_WORLD_DB=~/apfootballmanager/data/world.db \
cargo test --manifest-path src-tauri/Cargo.toml -p fm24-adapter --lib \
  career::tests::benchmark_real_career_cache -- --ignored --nocapture
```

Each "year N" row is a **worst case**: a new empty cache directory, so the builder copies the
snapshot and replays N seasons from the beginning. A real career advances one season at a time, so
only year 1 is representative of a first use, and every later season pays roughly the difference
between consecutive rows, not the whole rebuild.

The search excludes the 9,843 registry `active_world` identities. The production command excludes the
live roster and retired archive instead — about 13,045 imported ids — so this benchmark scans
slightly *more* rows than production, and its query timings are an upper bound rather than a
shortcut.

### 1.2 Results

Registry (unchanged from D7B1): stored 81,762, eligible 69,301, active-world 9,843,
72,048,640 bytes, built in 2,475 ms (2,528 ms including setup).

| Year | Season | Stored rows | Searchable | Newgens | Mean age | Free agents | OVR 80+ | OVR 85+ | Cache bytes | Cold rebuild | Cold query | Warm query |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 2027 | 73,530 | 61,069 | 900 | 26.5 | 45.0% | 0.00% | 0.00% | 58,597,376 | 2,280 ms | 52 ms | 52 ms |
| 5 | 2031 | 61,549 | 49,088 | 4,500 | 28.0 | 32.3% | 0.01% | 0.00% | 49,688,576 | 7,144 ms | 42 ms | 42 ms |
| 10 | 2036 | 44,482 | 32,021 | 9,000 | 29.2 | 34.8% | 0.13% | 0.00% | 35,745,792 | 12,976 ms | 29 ms | 29 ms |
| 20 | 2046 | 26,630 | 14,169 | 14,121 | 26.5 | 26.8% | 0.40% | 0.00% | 21,614,592 | 17,630 ms | 13 ms | 13 ms |
| 30 | 2056 | 26,649 | 14,188 | 14,188 | 26.5 | 27.5% | 0.36% | 0.01% | 21,626,880 | 19,943 ms | 13 ms | 13 ms |

Peak resident set size for the whole five-cache process: **1,751,384,064 bytes (~1.63 GiB)**; peak
memory footprint **1,390,151,768 bytes**. Two earlier identical runs measured 2,282,618,880 / 1.93 GiB
and 2,671,624,192 / 2.41 GiB, so the peak varies with SQLite page cache and allocator reuse across
five consecutive builds in one process. It is an upper bound, not the footprint of a single
production query.

### 1.3 What the real-source numbers say

* **The query path is unaffected by career age.** D7B1's search cost is a function of the number of
  rows the query scans; fewer rows make later seasons *faster* (52 ms → 13 ms). The cache does not
  add per-row projection work to a search.
* **The rebuild is linear in seasons replayed**, not in the number of seasons ever simulated:
  ≈ 0.7 s per replayed season at 900 newgens and up to 69k source rows, falling as the source
  population retires. A one-season advance on an existing cache is therefore sub-second to a few
  seconds.
* **The cache stays outside the save and shrinks as players retire.** 58.6 MB at year 1 → 21.6 MB
  at year 20+. It is never stored in `databases/` and does not travel with the save.
* **The searchable market contracts**, from 61,069 at season 1 to 14,169 at season 20, then
  stabilises at ≈ 14.2k. The 900/season intake is smaller than the source's retirement flow, which
  is deliberate (avoid generated-player inflation) but is a real product limit: after 20 seasons the
  external market is roughly the size of the active world, not a 69k worldwide pool.
* **There is a demographic bulge.** Mean age rises from 26.5 to 29.2 by season 10 (the imported
  cohort ages together) and returns to 26.5 once Newgens dominate. Free-agent share similarly runs
  at 32–35% in years 5–10 before settling at ≈ 27%. Neither is a shortage or universal free agency,
  but the first decade is visibly less balanced than the steady state.
* **Elite regeneration is effectively absent.** OVR 85+ is 0.00% in every real-source row and
  OVR 80+ never exceeds 0.4%. The imported source rows are the only elite players; the game's
  generated-player ceiling distribution does not replace them. This is the Active world's own
  generator behaviour, not a D7B2-specific rule, but it means the external market's top end decays
  over a long career.

### 1.4 Stored rows vs searchable rows

The stored cache keeps every source row — including `active_world`, `missing_identity`,
`missing_nation`, `no_position` and `outside_age_range` — so coverage stays inspectable and a
rebuilt cache can be compared row for row. Search applies the D7B1 filters:
`status = 'eligible'`, birth date inside `MAX_EXTERNAL_AGE`, and the live exclusion set. That is why
year 30 stores 26,649 rows but exposes 14,188: 9,843 active-world rows and ~2,618 non-eligible
source rows are stored, not offered. The numbers are consistent:
`26,649 − 9,843 − 2,618 = 14,188`.

## 2. Synthetic 30-season scenarios

`ofm_core::external_world::tests::thirty_season_markets_stay_populated_and_credible` runs 1,200
synthetic source players (ages 16–37, OVR 42–84, five clubs plus free agents) through 30 seasons
for three seeds, and asserts the long-career invariants after each run:

* no duplicate identity;
* population above half the starting count (no collapse);
* `ovr <= potential` for every player (no ceiling violation);
* at least one Newgen exists (replacement supply is real);
* free-agent share in `[2%, 75%]` (neither universal free agency nor a closed market);
* OVR 85+ share below 20% (no quality inflation).

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --lib \
  external_world::tests::thirty_season_markets_stay_populated_and_credible \
  -- --ignored --nocapture
```

Runtime: **9.19 s** for all three seeds × 30 seasons.

### 2.1 Population, age, free agents and quality

| Seed | Season | Population | Newgens | Mean age | Age ≤ 23 | Free agents | Mean OVR | OVR 85+ |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 101 | 2026 | 1,200 | 0 | — | — | — | — | — |
| 101 | 2031 | 5,198 | 4,500 | 21.4 | 82.8% | 27.0% | 59.7 | 1.0% |
| 101 | 2036 | 9,431 | 9,000 | 23.3 | 52.4% | 23.0% | 62.6 | 0.5% |
| 101 | 2041 | 13,225 | 13,046 | 25.3 | 37.7% | 25.4% | 63.2 | 0.2% |
| 101 | 2046 | 14,118 | 14,111 | 25.8 | 35.2% | 26.5% | 63.2 | 0.0% |
| 101 | 2051 | 14,198 | 14,198 | 25.9 | 34.8% | 26.9% | 63.2 | 0.0% |
| 101 | 2056 | 14,175 | 14,175 | 25.9 | 35.0% | 26.1% | 63.2 | 0.0% |
| 202 | 2031 | 5,208 | 4,500 | 21.4 | 82.5% | 25.3% | 59.8 | 1.0% |
| 202 | 2036 | 9,428 | 9,000 | 23.3 | 53.0% | 22.9% | 62.7 | 0.5% |
| 202 | 2041 | 13,189 | 13,022 | 25.3 | 37.3% | 25.3% | 63.3 | 0.2% |
| 202 | 2046 | 14,171 | 14,166 | 25.9 | 34.9% | 26.2% | 63.2 | 0.0% |
| 202 | 2051 | 14,119 | 14,119 | 25.8 | 35.1% | 26.0% | 63.1 | 0.0% |
| 202 | 2056 | 14,169 | 14,169 | 25.9 | 35.3% | 26.5% | 63.2 | 0.0% |
| 303 | 2031 | 5,200 | 4,500 | 21.4 | 82.3% | 27.7% | 59.7 | 0.9% |
| 303 | 2036 | 9,433 | 9,000 | 23.3 | 52.4% | 23.3% | 62.5 | 0.5% |
| 303 | 2041 | 13,249 | 13,061 | 25.3 | 37.4% | 24.8% | 63.2 | 0.2% |
| 303 | 2046 | 14,103 | 14,100 | 25.8 | 35.2% | 26.6% | 63.2 | 0.0% |
| 303 | 2051 | 14,147 | 14,147 | 25.8 | 34.9% | 26.4% | 63.2 | 0.0% |
| 303 | 2056 | 14,207 | 14,207 | 25.9 | 35.1% | 26.0% | 63.3 | 0.0% |

### 2.2 What the synthetic run proves and does not prove

* **Deterministic and seed-specific.** Seeds 101/202/303 produce the same *shape* (population,
  age mix, free share, mean OVR) but different identities, clubs, contracts and attributes. The
  five-season repeatability test asserts exact equality for one seed and inequality between two.
* **No collapse.** From 1,200 sources the market stabilises around 14.1–14.2k, driven by the fixed
  900/season intake. The retirements are not backfilled 1:1, so the population is intake-limited,
  not source-limited, after ~20 seasons.
* **Quality is stable, not inflated.** Mean OVR settles at 63.1–63.3 for all three seeds.
* **Free-agent share is stable.** 25–28% in the steady state, never universal free agency. The
  earlier ~53% reading was the contract-expiry bug, now covered by the expiry test.
* **The fixture, not the engine, explains the empty elite tier.** The synthetic source's maximum is
  84, so `ovr85+` can only come from Newgens; the game's generated ceilings almost never exceed 85.
  A 0.0% tail here is a fixture + generator property, not evidence about the real 2026 population
  (the real-source table shows the same end state, which is why it is listed as a limitation rather
  than hidden).

## 3. Determinism and persistence evidence

| Claim | Test |
|---|---|
| Same player-season projects identically | `external_world::tests::a_projection_is_identical_when_repeated` |
| Different seeds diverge | `external_world::tests::two_careers_with_different_seeds_diverge` |
| Five seasons: same seed equal, other seed different | `external_world::tests::five_seasons_are_repeatable_and_seed_specific` |
| Retirement never reverses | `external_world::tests::retirement_never_reverses` |
| Same season reuses the same cache rows | `career::tests::the_same_career_and_season_reuse_the_same_rows` |
| Two seeds build different markets | `career::tests::two_seeds_build_different_markets` |
| Advance never resurrects a retired player | `career::tests::advancing_never_resurrects_a_retired_player` |
| Clock moved backwards rebuilds from source | `career::tests::a_clock_moved_backwards_rebuilds_from_the_source` |
| Seed survives save/load (pre-existing) | `db::game_persistence::tests::a_games_seed_survives_a_save_and_load` |
| Excluded set covers roster + retired, `fm-p-*` and `fm-n-*` | `commands::registry::tests::the_context_excludes_roster_and_retired_source_identities` |
| Lowball rejected before mutation | `external_registry::tests::a_lowball_offer_is_rejected_before_any_mutation` |
| Counter deterministic and payable | `external_registry::tests::a_counter_is_deterministic_and_payable` |

## 4. Regression and contract tests run

| Suite | Command | Result |
|---|---|---|
| Rust workspace | `cargo test --manifest-path src-tauri/Cargo.toml --workspace` | **4,025 passed, 0 failed, 56 ignored** |
| Clippy, all targets | `cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets` | clean |
| Formatting | `cargo fmt --manifest-path src-tauri/Cargo.toml --all` | applied, no diff |
| Frontend suite | `npm test` | **310 files, 2,722 tests passed, 0 failed** |
| Frontend build | `npm run build` | succeeded (`tsc && vite build`) |
| Frontend lint | `npm run lint` | exit 0 (142 pre-existing `info` diagnostics, none in D7B2 files) |
| Frontend format | `npm run format:check` | clean, 791 files |
| Dead code (frontend) | `npm run knip` | one pre-existing unused export (`src/lib/communityLinks.ts`), not touched by D7B2 |
| 30-season multi-seed | ignored benchmark above | 9.19 s, invariants hold |
| Real-source cache | ignored benchmark above | 5 seasons + 30, invariants hold |

The 56 ignored tests are the project's pre-existing benchmarks plus the two D7B2 real-source /
long-career benchmarks (which need the source file or minutes of runtime).

After the concurrency hardening (`CACHE_MAINTENANCE` mutex + `concurrent_callers_see_one_complete_cache`),
the full Rust workspace suite was re-run from a clean compile and passed: **4,025 passed, 0 failed,
56 ignored**.

## 5. Not run, or not re-measured

Stated plainly so no unrun check is read as passing:

1. **Save size and save/load duration were not re-measured.** They are structurally unaffected: no
   `Game` field, table or migration was added, and the cache lives outside the save. The claim is
   architectural, not a fresh timing.
2. **A real 30-season playthrough in the Tauri app was not run.** The season harness does not drive
   the external cache; the cache is advanced lazily by the registry commands. The real-source
   benchmark calls the same `ensure_career_cache` the commands call.
3. **No cold-OS-cache measurement.** Query timings are warm page cache, single run.
4. **No external-club squad simulation was measured, because none exists** (by design).
5. **P5's worktree was not opened.** Compatibility is argued from the public surfaces in
   [`APFM_D7B2_INTEGRATION_CONTRACT.md`](APFM_D7B2_INTEGRATION_CONTRACT.md), not from P5's code.
6. **The concurrency test is a smoke test, not a proven race reproduction.** It runs two callers
   against a stale cache and asserts they agree; the mutex is hardening, validated by that test and
   by review of the single-staging-name failure mode.
