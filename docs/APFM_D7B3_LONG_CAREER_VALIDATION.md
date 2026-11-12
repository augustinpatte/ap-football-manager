# APFM D7B3 — Long-career validation

**Status:** locally validated, with a 2026-11-12 completion pass. **Date:** 2026-11-11.
**Branch:** `deepseek/d7b3-world-market`. **Base commit:** `ea469f3a`.

This document records what was measured, with which commands, and — as importantly — what was not
run. A local pass is not a claim about CI.

## 1. Environment and sources

| Item | Value |
|---|---|
| Platform | macOS (Darwin), Apple Silicon |
| Source database | `~/apfootballmanager/data/world.db` |
| Source SHA-256 | `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` |
| Save used for the P5 integration suite | `~/football-manager/2026-27 SEASON FREE SAVE GAME.fm` |
| Save SHA-256 | `70915fb59a6209e0b1a76789fe0ae912fac4aac9347a123b63350478ec018d4e` |

The source database is opened read-only and is never committed. No FM-derived data is added to the
repository; the benchmark stores its registry and career caches in `TempDir`s.

## 2. Real-source career-cache benchmark

Command (one seed per invocation; the registry is rebuilt inside the same temporary directory):

```bash
APFM_D7B3_SEED=<seed> \
APFM_D7B2_WORLD_DB="$HOME/apfootballmanager/data/world.db" \
cargo test -p fm24-adapter --lib career::tests::benchmark_real_career_cache -- --ignored --nocapture
```

The registry build produced, on every run:

- 81,762 stored rows;
- 69,301 eligible rows;
- 9,843 active-world rows excluded from the market;
- 72,179,712 bytes (72.2 MB);
- 2.5–4.1 s build.

Each career cache was built cold at seasons 1, 5, 10, 20 and 30. “Searchable” is the eligible
population after the projection; “Newgens” is the cumulative count of `fm-n-*` identities.

### 2.1 Population and quality

| Seed | Season | Rows | Searchable | Newgens | Avg age | Free | OVR 80+ | OVR 85+ |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 777 | 1 | 79,171 | 66,710 | 6,000 | 26.0 | 42.5% | 0.39% | 0.02% |
| 777 | 5 | 79,171 | 66,710 | 21,021 | 26.5 | 27.9% | 0.51% | 0.03% |
| 777 | 10 | 79,171 | 66,710 | 42,221 | 26.6 | 27.8% | 0.63% | 0.04% |
| 777 | 20 | 79,171 | 66,710 | 66,634 | 26.4 | 26.5% | 0.48% | 0.03% |
| 777 | 30 | 79,171 | 66,710 | 66,710 | 26.3 | 27.1% | 0.48% | 0.03% |
| 2026 | 1 | 79,139 | 66,678 | 6,000 | 26.0 | 42.3% | 0.38% | 0.02% |
| 2026 | 5 | 79,139 | 66,678 | 21,001 | 26.5 | 28.1% | 0.49% | 0.04% |
| 2026 | 10 | 79,139 | 66,678 | 42,201 | 26.6 | 27.8% | 0.60% | 0.05% |
| 2026 | 20 | 79,139 | 66,678 | 66,630 | 26.4 | 26.4% | 0.45% | 0.03% |
| 2026 | 30 | 79,139 | 66,678 | 66,678 | 26.3 | 26.7% | 0.41% | 0.03% |
| 9001 | 1 | 79,136 | 66,675 | 6,000 | 26.0 | 42.6% | 0.39% | 0.03% |
| 9001 | 5 | 79,136 | 66,675 | 20,997 | 26.5 | 28.0% | 0.51% | 0.03% |
| 9001 | 10 | 79,136 | 66,675 | 42,078 | 26.6 | 27.9% | 0.59% | 0.04% |
| 9001 | 20 | 79,136 | 66,675 | 66,616 | 26.4 | 26.4% | 0.47% | 0.04% |
| 9001 | 30 | 79,136 | 66,675 | 66,675 | 26.3 | 26.9% | 0.47% | 0.02% |

D7B2, for comparison, fell from 61,594 searchable at year 1 to 14,188 at year 30, with OVR 80+
falling from 0.42% to 0.36%. D7B3 is flat within one player per seed, and OVR 80+ is below 0.7% at
every measured horizon.

### 2.2 Build and search latency, cache size

| Seed | Season | Cold build ms | Cache bytes | Cold query ms | Warm query ms | Matches |
|---|---:|---:|---:|---:|---:|---:|
| 777 | 1 | 2,879 | 63,188,992 | 56 | 55 | 66,694 |
| 777 | 5 | 10,241 | 63,827,968 | 55 | 55 | 66,710 |
| 777 | 10 | 19,243 | 62,832,640 | 54 | 53 | 66,710 |
| 777 | 20 | 37,083 | 62,189,568 | 55 | 53 | 66,708 |
| 777 | 30 | 65,218 | 62,177,280 | 103 | 75 | 66,710 |
| 2026 | 30 | 82,917 | 62,173,184 | 55 | 56 | 66,676 |
| 9001 | 30 | 55,987 | 62,156,800 | 49 | 48 | 66,673 |

Interpretation:

- the cache stays ~62 MB at every horizon, against a 72.2 MB source registry;
- the first page query is 49–103 ms on the real source, warm 48–75 ms;
- a cold build replays every season from the source; the cost grows linearly with the career
  season, as expected. A cache that is already current is reused with zero replay;
- the incremental daily path is the warm cache, not the cold build.

### 2.3 Memory

The same benchmark run under `/usr/bin/time -l` reported:

- maximum resident set size: 1,536,720,896 bytes (~1.43 GiB);
- peak memory footprint: 91,374,240 bytes (~87 MiB).

The gap is mostly shared pages and the test harness. The meaningful working-set figure for the
feature is the peak memory footprint (~87 MiB) while building all five cold caches, including the
72 MB registry. This is a measurement of the benchmark process, not of an idle app.

## 3. Synthetic long-career scenarios

These are committed tests, not one-off scripts:

| Test | What it proves |
|---|---|
| `external_world::tests::intake_tracks_retirements_one_for_one` | Intake equals retirements until the 6,000 safety cap; no intake when nothing retires. |
| `external_world::tests::newgens_are_unique_and_repeatable` | A cohort is unique, stable for the same seed/season, and its elite prefix exists. |
| `external_world::tests::thirty_seasons_keep_a_populated_market` | A 30-season projection never collapses or explodes. |
| `external_world::tests::five_seasons_are_repeatable_and_seed_specific` | Two runs of the same seed agree; another seed differs. |
| `external_world::tests::thirty_season_markets_stay_populated_and_credible` | The 30-season market keeps credible age, free-agent and quality shares. |
| `external_world::tests::a_projection_is_identical_when_repeated` | Pure projection is deterministic. |
| `external_world::tests::two_careers_with_different_seeds_diverge` | The shared source is not one career's save. |

The real-source table in §2 is the independent confirmation that these models hold at registry
scale.

## 4. Cache determinism and save compatibility

New test:

- `fm24_adapter::career::tests::an_incremental_cache_matches_a_cold_build_at_the_same_season` —
  advances a career one season at a time from 2027 to 2036 and builds another career cold at
  2036. The two caches are compared on identity, OVR, potential, birth date, club, ownership,
  contract end, full attribute JSON, market value, wage, status and source hash. They are equal.

Existing tests still pass:

- `the_same_career_and_season_reuse_the_same_rows`;
- `advancing_never_resurrects_a_retired_player`;
- `a_clock_moved_backwards_rebuilds_from_the_source`;
- `concurrent_callers_see_one_complete_cache`;
- `commands::registry::integration_tests::an_older_save_rebuilds_the_cache_and_the_market_is_reproducible`;
- `commands::registry::integration_tests::a_recruit_survives_a_save_and_reload`.

The registry cache schema did not change. A D7B2 cache from the same source, seed and season is
reused. New serialized game fields are `#[serde(default)]`; no stored game shape changed.

## 5. Behavioural negotiation evidence

Backend (`ofm_core::external_registry::tests`):

| Scenario | Test |
|---|---|
| A free agent joins on standard terms | `a_free_agent_joins_on_the_standard_contract_without_a_fee` |
| A contracted player costs the asking price | `a_contracted_external_player_is_bought_at_the_asking_price` |
| A player cannot exist twice | `the_same_source_player_cannot_exist_twice` |
| Window open for contracted, not needed for a free agent | `a_contracted_player_needs_an_open_window_but_a_free_agent_does_not` |
| Unaffordable fee blocks before mutation | `an_unaffordable_fee_blocks_before_any_mutation` |
| A retired-age player is refused | `a_player_past_the_projected_retirement_age_is_refused` |
| Expired contract is free | `an_expired_contract_is_free` |
| A lowball offer is rejected before mutation | `a_lowball_offer_is_rejected_before_any_mutation` |
| A counter is deterministic and payable | `a_counter_is_deterministic_and_payable` |
| A low wage gets a counter and only the counter signs | `a_low_wage_gets_a_counter_and_only_the_counter_signs` |
| A player refuses a club far below his level without mutation | `a_player_refuses_a_club_far_below_his_level_without_mutation` |
| A club counter is not an agreement | `a_club_counter_is_not_an_agreement` |
| An agreed signing bonus is paid and budgeted | `an_agreed_signing_bonus_is_paid_and_budgeted` |
| Contract terms a contract cannot carry are refused | `invalid_negotiation_terms_are_refused` |
| A contracted purchase meets the 30-senior limit | `a_contracted_purchase_meets_the_thirty_senior_limit` |
| Promised places from agreed deals are counted | `a_contracted_purchase_counts_places_promised_to_agreed_deals` |
| The last senior place is still usable | `a_contracted_purchase_is_allowed_into_the_last_senior_place` |

P5 integration (`ofm_core/tests/transfers_tests/external.rs`) exercises the same path against the
generated P5 world, including the two-step counter.

Commands (`commands::registry::tests`):

- `negotiating_through_the_command_needs_the_counter_put_back_on_the_table` — a repeated preview
  re-rolls nothing, a counter finalize is refused with a key, and the countered terms sign;
- `invalid_negotiation_terms_reach_the_command_with_a_key`;
- `recruiting_through_the_command_mutates_the_active_game_once`;
- `a_contracted_player_recruited_through_the_command_names_his_club`;
- `a_full_senior_list_blocks_a_contracted_command_signing`.

Frontend:

- `globalMarketModel.test.ts` — every sort key and filter, page reset, paging ranges, counter and
  signing predicates;
- `GlobalMarketTab.test.tsx` — sort/direction/filter/page requests, profile load, agreed signing
  through the confirmation, counter acceptance, refusal and reason;
- `registryService.test.ts` — both negotiation commands and their payloads.

## 6. Gates actually run

| Gate | Command | Result |
|---|---|---|
| Backend workspace | `cargo test --manifest-path src-tauri/Cargo.toml --workspace` | pass (see §6.1) |
| Backend lint | `cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets` | pass, no warnings |
| Backend format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | pass |
| Frontend suite | `npm test` | 343 files passed, 1 skipped; 3,011 tests passed, 4 skipped |
| Frontend build | `npm run build` (`tsc && vite build`) | pass |
| Frontend lint | `npm run lint` (`biome lint --error-on-warnings`) | pass |
| Frontend format | `npm run format:check` (`biome format`) | pass |
| Dead code | `npm run knip` | pass (no unused exports) |
| i18n report | `npm run audit:i18n` | exit 0; all locales match the English key set |
| i18n gates | `localeCoverage`, `placeholderParity`, `frontendKeyCoverage` | pass |
| Registry ordering | `every_sort_direction_orders_the_page_the_screen_asks_for`, `every_search_filter_narrows_the_same_page` | pass |
| Cache determinism | `an_incremental_cache_matches_a_cold_build_at_the_same_season` | pass |
| Real-source benchmark | `benchmark_real_career_cache --ignored` | pass for seeds 777, 2026 and 9001 |

### 6.1 Backend suite count

See the final report. The suite includes the existing ~4,964-test baseline plus the D7B3 ordering,
population, negotiation, cache and command tests; no existing test was removed or weakened.

### 6.2 Completion pass (2026-11-12)

The 30-senior rule for a contracted external purchase was added after the gates above, so the
full workspace suite was not rerun. Only Rust changed; the final tree was checked with:

| Gate | Command | Result |
|---|---|---|
| `ofm_core`, unit and integration | `cargo test -p ofm_core` | 3,221 passed, 0 failed, 16 ignored |
| Root library | `cargo test --lib` | 584 passed, 0 failed, 10 ignored |
| `fm24-adapter` library | `cargo test -p fm24-adapter --lib` | 101 passed, 0 failed, 2 ignored |
| Changed-crate lint | `cargo clippy -p ofm_core -p openfootmanager --all-targets` | pass, no warnings |
| Format | `cargo fmt --all` | pass |
| Frontend lint and format | `npm run lint`, `npm run format:check` | pass |
| Dead code | `npm run knip` | pass |
| i18n report | `npm run audit:i18n` | exit 0 |
| Frontend build | `npm run build` | pass |
| Global market + service tests | `vitest run ...GlobalMarket*, registryService.test.ts` | 18 passed |
| i18n gates | `vitest run src/i18n` | 35 passed |

The full frontend suite and the full workspace `cargo test` were not rerun; their 2026-11-11
results are in §6. No frontend file changed in the completion pass.

## 7. Not run

- **Desktop GUI validation was not performed.** No desktop-control tool was available in this
  environment, so the Global market tab was not clicked in a running Tauri window. The screen is
  covered by component tests and the production build, but that is not the same as a manual pass.
- **No end-to-end Tauri IPC test** drove the new commands from a real webview. The commands are
  covered by `cargo test --lib commands::registry` and the service wrappers by Vitest.
- **No long-running app memory profile.** The memory figure is a benchmark-process measurement.
- **No 44,032-club simulation** was attempted, by design. External clubs remain data, not
  simulated squads.
- **CI was not run.** A local pass does not guarantee CI.

## 8. Reproduction

```bash
npm ci
npm test
npm run build
npm run lint
npm run format:check
npm run knip
npm run audit:i18n
cargo test --manifest-path src-tauri/Cargo.toml --workspace
cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check

# Real-source benchmark (slow, needs the private database):
APFM_D7B3_SEED=777 \
APFM_D7B2_WORLD_DB="$HOME/apfootballmanager/data/world.db" \
cargo test -p fm24-adapter --lib career::tests::benchmark_real_career_cache -- --ignored --nocapture
```
