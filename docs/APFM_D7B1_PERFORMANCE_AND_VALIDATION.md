# APFM D7B1 — Performance and validation

**Status:** locally measured and validated. **Date:** 2026-11-11. **Branch:**
`deepseek/d7-database-audit`.
**Companion:** [`APFM_D7B1_GLOBAL_REGISTRY.md`](APFM_D7B1_GLOBAL_REGISTRY.md) — architecture,
identity, recruitment rules and known limitations. This document records what was measured, what
was proven by tests, and what was **not** run.

All numbers below come from one machine, one source save and one build of this branch. They are
evidence that the design behaves as described; they are not CI thresholds and should not be read
as guarantees for every host.

---

## 1. Environment and method

| Item | Value |
|---|---|
| Host | macOS 27.0, Apple M4, arm64 |
| Toolchain | rustc / cargo 1.95.0 |
| Rust test profile | `[profile.test] debug = 0`, `incremental = false` (optimised test binaries) |
| Real source | `~/apfootballmanager/data/world.db`, 241,463,296 bytes, SHA-256 `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` |
| Source save | FM 24.3.0, 2026/27 season, SHA-256 `70915fb59a6209e0b1a76789fe0ae912fac4aac9347a123b63350478ec018d4e` |
| Shipped active world | 240 clubs, 13,157 players, 12 leagues / 6 cups |
| Build-time reference date | 2026-08-01 for every query in this document |

**Real-source method.** A temporary integration probe built the registry from the real
`world.db` with `build_registry`, opened it read-only with `open_registry`, and timed
`query_registry` / `get_registry_player` with `std::time::Instant`. The probe was removed after
measuring: the source is the player's file and neither it nor a fixture derived from it is
committed. The same code path is what the five Tauri commands call. Build and coverage numbers are
also produced by the committed adapter tests against synthetic data, but the counts and byte sizes
below are from the real source.

**Scaling method.** The committed ignored benchmark
`fm24_adapter::registry::tests::large_registry_query_benchmark` writes synthetic registries at
13,157, 40,000 and 81,762 rows, excludes the first 12% (the share of the real source the active
world already owns), and times a name lookup, a value-sorted page and a free-agent page. It uses
no real or FM-derived data:

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p fm24-adapter --lib large_registry \
  -- --ignored --nocapture
```

**Caveats.** Timings are single runs with a warm OS page cache and the same SQLite connection per
query; they include the `total` count the screen needs, not just the page. They are indicative,
not a cold-start or worst-case measurement. The synthetic rows carry no attribute JSON or journal
bulk, so the synthetic file is less than half the real registry's size and its query times are a
lower bound for real data.

---

## 2. Real-source build and coverage

`build_registry` reads every source player through `Source::every_player()`, classifies and
converts each row with the same `convert_player` the world import uses, writes a staging file in one
transaction and atomically renames it into place.

| Metric | Real source |
|---|---:|
| Players seen / stored | 81,762 / 81,762 |
| Eligible external players | 69,301 |
| Free agents at snapshot | 19,540 |
| Already in the active world (`active_world`) | 9,843 |
| Missing identity (no usable name or birth date) | 145 |
| Missing OFM-mapped nation | 1,880 |
| No usable position | 0 |
| Outside the 14–40 build age band | 593 |
| Journal entries | 81,762 |
| Build time | **2,500 ms** |
| Registry file | **72,048,640 bytes (68.7 MiB)** |
| Peak RSS during build (test process) | **402,587,648 bytes (384 MiB)** |

No source player is dropped: every row is stored with a status, an exclusion reason where
applicable, and a journal entry. The counts add up to the stored total
(69,301 + 9,843 + 145 + 1,880 + 0 + 593 = 81,762).

**Value and contract provenance among stored rows:**

| Field | Value | Count |
|---|---|---:|
| `value_source` | `save` | 53,128 |
| | `estimated` (source had no value) | 26,144 |
| | `générée` (source-generated value) | 2,490 |
| `wage_source` | `save` | 60,338 |
| | `générée` | 976 |
| | empty / not recorded | 20,448 |
| Contract among contracted rows | running | 58,069 |
| | no contract end recorded | 3,209 |
| | expired at the reference date | 36 |

The build is one-off at install time, runs inside `spawn_blocking`, and cannot damage the previous
registry: a failed build leaves `registry.db` untouched. It is not on the daily turn loop.

---

## 3. Real-source search latency

Queries use page size 25, the default the service sends. `excluded` is the realistic case: 9,843
`fm-p-*` identities the active career already owns, so they are filtered out of results and counts.

| Query | With 9,843 exclusions | Without exclusions |
|---|---:|---:|
| Any eligible player, sorted by name | 61.3 ms | 37.2 ms |
| Name search (`silva`), sorted by name | 65.0 ms | 40.5 ms |
| Any eligible player, sorted by value | 61.7 ms | — |
| Free agents only, sorted by value | 90.6 ms | — |

The exclusion list doubles the cost because it is a large `NOT IN` set evaluated alongside the
filter and the `total` count; even so every realistic page returns in under 100 ms on this host,
and the frontend still receives only 25 rows.

The slowness of the free-agent query is the contract projection, not the page: ownership depends
on the career date and the row's contract end, so the filter is evaluated per row before counting.
It remains under 100 ms at full source scale and is bounded by the number of eligible rows, not by
the whole source.

---

## 4. Scaling: 13,157 / 40,000 / 81,762

Synthetic registries, 12% of identities excluded, reference date 2026-08-01.

| Rows | Build | File | Excluded | Name lookup | Value sort | Free agents |
|---:|---:|---:|---:|---:|---:|---:|
| 13,157 | 59 ms | 5,058,560 B (4.8 MiB) | 1,578 | 8.0 ms | 6.8 ms | 9.9 ms |
| 40,000 | 192 ms | 15,708,160 B (15.0 MiB) | 4,800 | 25.7 ms | 24.9 ms | 37.0 ms |
| 81,762 | 438 ms | 32,612,352 B (31.1 MiB) | 9,811 | 52.9 ms | 52.3 ms | 78.0 ms |

The whole three-scale benchmark process peaked at **15,925,248 bytes (15.2 MiB) maximum RSS**,
because each registry is built, queried and dropped in turn. Query time grows roughly linearly with
the eligible row count; it does not grow with page size or with the number of rows sent to the
screen.

The real-source build is slower than the synthetic 81,762 build (2,500 ms vs 438 ms) because the
synthetic writer skips source reading, conversion and journal attributes. The real file is also
more than twice the synthetic one (68.7 MiB vs 31.1 MiB) for the same row count. The synthetic
timings are therefore a floor, and the real-source table in §3 is the number that matters.

---

## 5. Memory, disk and save footprint

| Surface | Measured |
|---|---|
| Registry build, real source, peak RSS | 384 MiB (test process, one-off) |
| Three-scale synthetic benchmark, peak RSS | 15.2 MiB |
| Real registry on disk | 68.7 MiB under `<app data>/fm24-import/` |
| Synthetic 81,762-row registry | 31.1 MiB |
| Bytes added to the repository or build | 0 — the sidecar is generated on the player's machine |
| Bytes added to a save by the registry itself | 0 — no `Game` field, no SQL migration, no table |
| Bytes added by one promoted player | one normal row in the existing `players` table |

The registry is opened read-only for the duration of one command and dropped; it is not resident in
`Game`, not loaded at startup and not held across an `await`. The three-scale benchmark's 15.2 MiB
is the strongest evidence that the query path itself is small; the 384 MiB real build is dominated
by reading and converting the 241 MB source and is paid once at install, off the UI thread.

No `world.db`, registry, generated world or FM-derived fixture is committed. `distribution_guard.rs`
and the package tests are untouched.

---

## 6. What the frontend receives

Measured by serialising the real response with `serde_json`:

| Response | JSON bytes |
|---|---:|
| One page of 25 summaries + totals + counts | **7,571 (7.4 KiB)** |
| One summary row | 278 |
| One full detail record (all attributes) | 988 |

The screen never receives the registry. A page is bounded by the backend clamp (1–100 rows,
default 25), so a hostile or buggy request cannot pull the 69,301 eligible records into the
renderer. The six new error keys add a negligible, fixed amount to each locale bundle.

---

## 7. Daily, season-rollover and save/load overhead

**Daily turn loop: zero registry work.** There is no reference to the registry or
`ofm_core::external_registry` under `ofm_core/src/turn/` or `engine/src/`, and `commands/time.rs`
does not touch it. The registry is reached only by the five registry commands. No nightly job,
no per-day scan, no external-club simulation.

Everything that changes with time is a projection evaluated on read, once per row considered by a
query:

* age from birth date and `game.clock`;
* free agent vs contracted from `contract_end` against today;
* availability/retirement from the projected age (older than 37 is refused);
* asking price from market value and contract end.

That is O(1) per row, deterministic for a given source and date, and unbounded only by the query's
matching set — not by elapsed days.

**Season rollover: zero registry work.** Season rollover does not import, age, retire or move an
external record. The registry's answer changes only because the career date moved, which the next
query recomputes. There is no rollover benchmark because the registry is not on that path; the
existing season-harness and `cargo test --workspace` cover the rollover code that does run.

**Save / load: one normal player, no new persistence.** A promoted player is written by the
existing `players` repository, with attributes, contract, movement history and stats JSON. The
round-trip test `a_promoted_external_player_survives_a_save_and_load` writes a career after a
promotion, reads it back with a fresh reader and asserts:

* exactly one player under `fm-p-77` — one source identity, one player;
* the club, contract club, natural position, alternate positions and the free-agent movement entry
  survive;
* promoting the same source record again returns `be.error.registry.playerAlreadyInWorld`.

The registry file itself is not saved or loaded; it is rebuilt from source. Save format, schema
version and migration count are unchanged.

---

## 8. Validation matrix

Every row was run on this branch unless the result column says otherwise.

| Requirement | Evidence | Result |
|---|---|---|
| Identity is the source person id, not the FM UID | `registry::tests::ofm_ids_come_from_the_source_person_id_not_the_fm_uid` | pass |
| Every source player is stored and classified, none silently dropped | `registry::tests::every_source_player_is_stored_and_classified`; real counts in §2 add to 81,762 | pass |
| Records missing identity/nation/position/age are explicit | same test; real counts 145 / 1,880 / 0 / 593 | pass |
| Incomplete records are searchable only when explicitly requested and cannot be signed | `registry::tests::incomplete_records_are_searchable_only_when_explicitly_requested`, `commands::registry::tests::an_incomplete_detail_is_refused_with_a_key` | pass |
| Search by name, nation, position group, age, club; pagination; sort | `registry::tests::search_filters_and_paginates_eligible_players` | pass |
| Already-owned identities never appear, and counts follow the exclusions | `registry::tests::excluded_ids_never_appear_and_counts_follow_them`; `commands::registry::tests::the_context_excludes_roster_and_retired_source_identities` | pass |
| A detail row round-trips into a promotion payload | `registry::tests::a_detail_round_trips_into_the_promotion_payload` | pass |
| A missing registry is a key, not a silently created empty database | `registry::tests::reading_a_missing_registry_fails_with_a_key_and_does_not_create_it` | pass |
| A rebuild is content-identical | `registry::tests::a_rebuild_is_content_identical` | pass |
| Free agent: standard contract, no fee | `external_registry::tests::a_free_agent_joins_on_the_standard_contract_without_a_fee` | pass |
| Contracted external player: bought at the deterministic asking price | `external_registry::tests::a_contracted_external_player_is_bought_at_the_asking_price` | pass |
| One source player can never exist twice | `external_registry::tests::the_same_source_player_cannot_exist_twice`; `commands::registry::tests::recruiting_through_the_command_mutates_the_active_game_once` | pass |
| Contracted players need an open window; free agents do not | `external_registry::tests::a_contracted_player_needs_an_open_window_but_a_free_agent_does_not` | pass |
| An unaffordable fee blocks before any mutation | `external_registry::tests::an_unaffordable_fee_blocks_before_any_mutation` | pass |
| A player past the projected retirement age is refused | `external_registry::tests::a_player_past_the_projected_retirement_age_is_refused` | pass |
| An expired contract is treated as a free agent | `external_registry::tests::an_expired_contract_is_free` | pass |
| The search command takes its clock and page policy from the career | `commands::registry::tests::a_search_request_takes_its_clock_from_the_career` | pass |
| A contracted promotion through the command names the external club and puts the player on the roster | `commands::registry::tests::a_contracted_player_recruited_through_the_command_names_his_club` | pass |
| Save / reload keeps one promoted player and refuses a second promotion | `db::game_persistence::tests::a_promoted_external_player_survives_a_save_and_load` | pass |
| The frontend service sends the exact invoke contract for search, detail, preview, recruit and status | `src/services/registryService.test.ts` (4 tests) | pass |
| All 12 locales carry the six new keys, with no orphans or untranslated strings | `src/i18n/localeCoverage.test.ts`, `src/i18n/frontendKeyCoverage.test.ts`, `src/utils/backendI18n.localeCoverage.test.ts` | pass |
| Layering and import direction | `src/architecture.test.ts`, `src-tauri/tests/architecture.rs` (inside the suites below) | pass |
| Install response carries registry coverage and a key when the registry failed | `commands::fm24_world::tests::a_world_installs_even_when_the_registry_fails`; `src/services/worldDatabaseService.ts` type-checks in `npm run build` | pass |

---

## 9. Checks run

| Check | Command | Result |
|---|---|---|
| Rust formatting | `cargo fmt --manifest-path src-tauri/Cargo.toml --all --check` | pass |
| Rust workspace tests | `cargo test --manifest-path src-tauri/Cargo.toml --workspace` | pass — all targets, 0 failures |
| Rust lints, all targets | `cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets` | pass — no warnings |
| Frontend tests | `npm test` | pass — 310 files / 2,722 tests |
| Frontend production build + type-check | `npm run build` | pass |
| Frontend formatting (Biome) | `npm run format:check` | pass — 791 files, no fixes |
| Frontend lint (Biome) | `npm run lint` | pass — exit 0, 142 pre-existing infos, none in the new files |
| Unused files / dependencies (knip) | `npm run knip` | one pre-existing unused export (`DISCORD_INVITE_URL`), unrelated to D7B1; the new service is not reported |
| Real-source build and coverage | temporary local probe against the source hash in §1 | pass — §2 |
| Real-source query latency | same probe | pass — §3 |
| Scaling benchmark, 3 sizes | committed ignored benchmark | pass — §4 |
| Payload sizes | serialised real response in the probe | pass — §6 |

---

## 10. Checks not run, and residual risk

These are deliberately **not** claimed as passed:

1. **Manual install through the running Tauri app.** The install command compiles, builds the
   world first and then the registry in `spawn_blocking`, and returns coverage plus
   `registryError`; no human run of the install dialog was performed in this worktree. The
   registryFile path and status command are therefore proven by code and unit-level wiring, not by
   an end-to-end GUI session.
2. **Windows and Linux.** Path handling, SQLite page behaviour and peak memory were measured on
   macOS/arm64 only.
3. **Cold cache.** Query latency was measured with a warm page cache; the first query after a
   reboot may be slower.
4. **Release-profile timings.** Numbers use the optimised test profile, not a `--release` app build.
5. **Concurrent GUI + MCP load.** Each command opens the registry read-only and drops it, and no
   lock is held across `await`, but no contention test was run.
6. **A UI screen.** D7B1 ships no screen, so no accessibility or visual review of a registry screen
   exists. The six error keys are translated now; the screen review is future work.
7. **MCP exposure.** The five commands are Tauri commands only; no MCP tools were added, so there is
   no MCP contract or tool test for the registry.
8. **Other saves.** Real coverage comes from one FM 24.3.0 save. A different save can change the
   counts and the share already owned by the active world.
9. **Staleness detection.** A changed `world.db` is not detected beyond the recorded SHA-256; the
   supported correction is a rebuild. There is no automatic invalidation.
10. **Long-running soak / many seasons.** External records are projections, not simulated entities;
    there is no soak test because there is no external state to drift. Promoted players are normal
    players and are covered by the existing season and persistence suites.
11. **`cargo-machete`.** Not run. The frontend repository-wide checks were run (§9); `knip` reports
    one pre-existing unused export outside D7B1. No dependency was added to any crate by this
    change, so machete is not expected to move, but it was not executed.

The largest residual product risk is not performance: it is that D7B1 deliberately freezes external
attributes and runs no external-club AI, so a signed player's ability does not reflect the season
he was scouted in, and an external club whose player is bought simply loses him. Both are named in
the architecture document §9 and are prerequisites for a later staged expansion, not hidden costs
of this one.

---

## 11. Verdict

* **GO** for the D7B1 pathway as delivered: an indexed, read-only, local sidecar that makes the
  eligible source players searchable and lets the manager promote one into the active world, with
  one identity, one contract, deterministic pricing and save/reload correctness.
* **NO-GO** for any claim that all 81,762 source players are simulated or recruitable. 69,301 are
  eligible, 12,461 are explicitly classified as unusable for recruitment, and external clubs and
  competitions are records rather than simulated entities.
* The measured cost of the registry is a one-off 2.5 s / 69 MiB build at install and a bounded
  per-query read (§3–§4). The daily turn loop, season rollover and save format are untouched.
