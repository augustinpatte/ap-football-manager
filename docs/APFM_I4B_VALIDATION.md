# I4B — Validation

Evidence for [`APFM_I4B_WORLD_COMPETITIONS.md`](APFM_I4B_WORLD_COMPETITIONS.md). Every run below
was executed on this branch, not inferred from a previous report. Commands and results are copied
from the run logs, which are kept outside the repository.

## 1. Environment and revision

| | |
| --- | --- |
| Branch | `deepseek/i4-world-competitions` |
| Base (`0dc3582dc8a4c0a7b7a87d6746377c7618f90d36`) | `0dc3582d` |
| Toolchain | `rust-toolchain.toml` (1.95.0), macOS arm64 |
| Test profile | `cargo test` default `test` profile, 8 test threads |
| Clock of record | 2026-11-15 |

`git merge-base --is-ancestor 0dc3582d HEAD` passes; the branch is a linear descendant of the
required base and no commit from another worktree is included.

## 2. The defect, before and after

### 2.1 Production symptom

Before this change the only season-transition trigger was `process_end_of_season`, called when the
manager's league finished. A foreign league finished between two rollovers had no completion hook:

* Argentina's Apertura (1 February opener) was regenerated at the English rollover anchor in
  April/May, so every archived edition opened months late.
* A league that finished after the rollover missed the whole next edition and sat idle until the
  following rollover.

The integration study `foreign_leagues_keep_their_authored_calendar_and_archive_every_edition_once`
asserts the Apertura opens on `(2, 1)` in every archived season, that archived seasons are
consecutive, and that no edition sits more than 150 idle days. It fails on the baseline for the
opener-drift finding; the baseline run is recorded in §7.

### 2.2 Unit-level red/green

| Test | Without the fix | With the fix |
| --- | --- | --- |
| `a_berth_reads_the_archived_table_of_a_country_renewed_early` | `left: ["new-leader"]`, `right: ["old-champion"]` | pass |
| `a_cup_already_renewed_for_next_season_does_not_hold_the_rollover` | `is_season_complete` returned `false` (the cup's unplayed next edition held it open) | pass |

The first was run by temporarily restoring the old `played > 0` predicate in
`standings_for_berth`; the second by removing the `competition.season <= settling_season` guard, so
the cup's already-renewed next edition blocked the rollover again. Both were then restored and
re-run green. No assertion was weakened to obtain either red state.

## 3. Tests added or changed

### 3.1 OFM core unit tests — `turn::country_renewal::tests`

Nine tests, all green in isolation and in the full lib run:

| Test | Scenario |
| --- | --- |
| `a_finished_foreign_country_renews_on_its_own_authored_opener` | Two-tier country, opener four weeks away: both tiers, P/R and one archive per tier |
| `a_renewed_country_is_not_renewed_or_paid_twice` | Next day does nothing; one prize and one history row per club |
| `the_managers_own_country_waits_for_the_rollover` | User country untouched |
| `a_berth_fed_country_waits_for_the_rollover` | Authored domestic berth defers the country |
| `a_country_with_a_tier_still_playing_is_left_alone` | Whole country waits for the last rung |
| `a_split_season_country_renews_each_half_on_its_own_opener` | Apertura opens in February and does not pay; Clausura opens in July and pays the year once |
| `a_missed_opener_is_recovered_on_a_short_calendar` | Pre-hook save: 21-day recovery, never a year-long idle |
| `a_game_without_a_manager_does_not_renew_foreign_countries` | Sandbox guard |
| `processing_a_day_renews_a_due_foreign_country` | The hook is wired into `process_day` |

### 3.2 `end_of_season::season_calendar::tests`

Five edition-choice tests: authored opener kept, long-passed opener skipped to the following one,
just-passed opener recovered soon, leap-day clamping, and no edition starting on the day the hook
runs.

### 3.3 Rollover and berth tests

* `a_cup_already_renewed_for_next_season_does_not_hold_the_rollover` — new.
* `a_berth_reads_the_archived_table_of_a_country_renewed_early` — new.
* `a_berth_reads_the_live_final_table_of_the_season_being_settled` — new, pins the other side of
  the discriminator so the fix cannot silence a live final table.
* Existing rollover, cup-renewal, archive and promotion/relegation tests were kept unchanged and
  pass.

### 3.4 Season-harness integration tests — `tests/foreign_calendar.rs`

| Test | Seasons | Result |
| --- | --- | --- |
| `foreign_leagues_keep_their_authored_calendar_and_archive_every_edition_once` | 3 | pass, 2-test run 20.8 s |
| `a_reloaded_career_keeps_the_renewed_calendar` | 3, SQLite round trip every season | pass, same run |
| `thirty_seasons_of_foreign_calendars` | 30 | pass, 226.1 s |
| `imported_world_keeps_foreign_calendars_for_five_seasons` | 5 on the 240-club FM24 import | see §5 |

The gate world is England (manager), Portugal (ordinary single tier, November finish) and Argentina
(split season, February/July halves, same clubs). It is played through the production day and
rollover paths, with the harness's every-day and every-rollover invariants: referential integrity,
no stranded fixture, result presence, standings consistency, club rest, ladder holding, competition
survival, one-season advancement, fresh tables, season-stat reset and finished ladders moving clubs.

## 4. Thirty-season stress study

`thirty_seasons_of_foreign_calendars`, gate world, seed 1, production paths.

```
thirty gate seasons in 226.1s
  season  1: 5.3s
  season  2: 6.2s
  season  3: 6.7s
  season  5: 6.7s
  season 10: 7.0s
  season 15: 7.6s
  season 20: 7.5s
  season 25: 7.7s
  season 30: 13.0s
archived editions: 278 across 9 competitions
test result: ok. 1 passed
```

Process measurements from `/usr/bin/time -l`:

| Metric | Value |
| --- | --- |
| Wall time | 226.3 s (no compilation in this run) |
| User CPU | 224.4 s |
| Maximum resident set size | 654.4 MB |
| Peak memory footprint | 91.4 MB |
| Swaps | 0 |

The per-season cost grows from 5.3 s to 7.7 s over 25 seasons, with a thirtieth-season spike that
varies between runs (7.5 s and 13.0 s in two consecutive runs of the same code). The growth is
consistent with the archive and career state growing (278 editions), not with a per-day scan that
grows with the number of competitions: the hook reads the competition list once a day and does work
only on a due country. It is recorded here as a limitation to watch, not as a gate failure.

The study asserts, after every season, that calendar findings are empty: consecutive archived
seasons, one archive per season, authored opener month and year, and no gap over 150 idle days.
It also asserts the harness invariants are clean and the lifecycle is clean.

## 5. Imported FM24 world

### 5.1 Provenance and freshness

The world is **not** in the repository. The study reads `APFM_WORLD_JSON` and skips when unset.
The file used:

```
/Users/augustinpatte/apfm-migration/output/m7world/apfm-fm24-2026.json
sha256 22d9e216d37f37efdecf8c8067234b40db5aa9fb9cad82b6f493c11623d4865e
```

The digest matches the file's own `SHA256SUMS`. `ADAPTER_REPORT.md` records the import source:
`world.db` sha256 `36b6908c…`, importer schema `1.1.0`, FM save `24.3.0`, 2026 season, 240 clubs,
12 league competitions and 6 national cups, 10,045 contracted players. This is the `m7world`
generation (the one with cups), not the older `m4world` file and not the repository's stale
`databases/` fixture.

### 5.2 Result

`imported_world_keeps_foreign_calendars_for_five_seasons` passes, five seasons, manager at PSG:

```
  imported season 1: 35.0s
  imported season 2: 48.6s
  imported season 3: 48.9s
  imported season 4: 43.7s
  imported season 5: 36.2s
imported world: 240 clubs, 18 competitions, 92 archived editions in 212.4s
test result: ok. 1 passed; 0 failed; 0 ignored; 3 filtered out; finished in 212.87s
```

`/usr/bin/time -l`: 213.2 s wall, 204.6 s user CPU, 419.8 MB maximum resident set size, 91.1 MB
peak memory footprint, no swaps.

After every season the study asserts empty calendar findings (consecutive archived seasons, one
archive per season, authored opener month and year, no idle gap over 150 days) and empty
fixture-identity findings (one owner per fixture id, the fixture names its competition, no club
plays itself). It then asserts at least eight foreign divisions renewed at least once, and that the
harness invariants and lifecycle remain clean. The two national-cup rounds of the import are played
through the existing cup paths; this work does not change cup scheduling.

The per-season cost wobbles (35–49 s) with the imported world's fixture volume and the number of
saves/rollovers it performs; there is no upward trend over five seasons.

## 6. Performance and save compatibility

* The hook is O(competitions) per day: one grouped scan, and the archive/P/R/regeneration work only
  on the days a country is due (at most once per country per season).
* No new serialized field; no migration. Old saves load with the same schema.
* Save/reload is exercised at every rollover in the 3-season gate study and at every season in the
  5-season imported study, comparing the full world fingerprint before and after (the only line
  excluded is the session-only squad-floor top-up log).
* The 30-season study compares nothing across process restarts; persistence across restarts is the
  reload study's job and it passes.

## 7. Baseline evidence

The integration test was compiled and run against the untouched I3 base in a disposable worktree at
`0dc3582dc8a4c0a7b7a87d6746377c7618f90d36`, with the branch's `tests/foreign_calendar.rs` copied in
unchanged (it only uses public API). Sharing this branch's `target` directory:

```
thread 'foreign_leagues_keep_their_authored_calendar_and_archive_every_edition_once' panicked at
crates/season-harness/tests/foreign_calendar.rs:152:5:
ar-d1-apertura: season 2034 opens on 2034-05-16 instead of its authored month 2
ar-d1-apertura: season 2035 opens on 2035-05-16 instead of its authored month 2
ar-d1-apertura: season 2033 ended 2033-10-22 and season 2034 only opens 2034-05-16 (206 idle days)
test result: FAILED. 0 passed; 1 failed
EXIT=101
```

Three findings, one root cause: with only the manager's rollover driving renewal, the Argentinian
Apertura is regenerated from the April/May rollover anchor, so its archived editions open in May
instead of February and it sits 206 days idle. On this branch the same test passes with zero
findings; the worktree was removed afterwards.

## 8. Validation runs

All Rust commands were run from `src-tauri/` on 2026-11-15/16, sequentially (no concurrent builds or
studies). The frontend is untouched, so no `npm` command was needed or run for this change.

| Check | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --all -- --check` | clean, exit 0 |
| Clippy, CI scope | `cargo clippy --workspace --all-targets -- -D warnings` | clean, exit 0 |
| Clippy, MCP features | `cargo clippy --features mcp --all-targets -- -D warnings` | clean, exit 0 |
| Backend suite | `cargo test --workspace` | 153 targets, 5,074 passed, 0 failed, 123 ignored, exit 0 |
| Rollover integration | `cargo test -p ofm_core --test end_of_season_tests` | 115 passed, exit 0 |
| Gate study | `cargo test -p season-harness --test foreign_calendar` | 2 passed, 2 ignored (studies), 20.8 s |
| 30-season study | `cargo test -p season-harness --test foreign_calendar thirty_seasons_of_foreign_calendars -- --ignored --nocapture` | 1 passed, 226.1 s |
| Imported study | `APFM_WORLD_JSON=… cargo test -p season-harness --test foreign_calendar imported_world_keeps_foreign_calendars_for_five_seasons -- --ignored --nocapture` | 1 passed, 212.9 s |
| Baseline | gate study in a worktree at `0dc3582d` | 0 passed, 1 failed (expected) |

The 123 ignored tests are the workspace's existing `#[ignore]`d studies and network/adapter tests;
the count is unchanged by this work apart from the two studies added here, which are ignored so a
plain `cargo test` stays fast and deterministic.

Two long-run observations, recorded rather than hidden:

* One earlier `cargo test --workspace` attempt was interrupted by an internal failure
  (`be.error.gamePersistence.writeFailed; cleanup=rollback_release_savepoint: no such savepoint:
  league_upsert`) in an unrelated save test. It did not recur in the complete run above, and that
  test passes in isolation. It is a pre-existing persistence path, not touched here.
* The studies are CPU-bound (user CPU ≈ wall time). They were run one at a time.

## 9. What is not claimed

* No person played the desktop UI for this work.
* Continental fields and the World Cup are still resolved at the rollover / world-cup calendar, not
  per country; §8 of the implementation record states the limitation.
* A foreign pyramid fed by authored domestic berths still waits for the rollover. The shipped and
  imported worlds use the generic ladder.
* The imported study is five seasons (the brief's minimum), not ten. It is a real 240-club,
  18-competition career with save/reload, not a registry-only study.
