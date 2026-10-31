# APFM — consolidation of Runs 1, 2, 3 and 4

31/10/2026. Branch `consolidation/run1-4`. Pull request towards `apfm/main`, **not merged**.
No branch was force-pushed or rewritten; every existing branch is untouched.

## 1. Branches and commits united

| Ref | Head | What it is |
|---|---|---|
| `apfm/main` | `4e5dd35` | Common ancestor of everything below (schema v061). |
| `claude/pensive-franklin-1g8n0r` | `6ac31fe` | Run 1 work branch (transfers). Ancestor of `integration/run1-run2`. |
| `integration/run1-run2` | `e36c7be` | Run 1 as integrated. |
| `codex/run2-tactics` | `faf9144` | Run 2 (tactics), 2 commits. |
| `claude/run3-integration` | `97939e4` | **Run 3**: `integration/run1-run2` + `codex/run2-tactics` merged, plus the v062/v063 migration reconciliation and the Run 2 development-save repair. |
| `run4-season-engine` (PR #3) | `306d4fa` | Run 4: season lifecycle, one commit on `4e5dd35`. |
| **`consolidation/run1-4`** | this branch | `claude/run3-integration` + `run4-season-engine` + the fixes below. |

Verified, not assumed (`git merge-base --is-ancestor`): `integration/run1-run2`, `codex/run2-tactics`
and `claude/pensive-franklin-1g8n0r` are all ancestors of `claude/run3-integration`, so it holds Runs 1 and 2
in full. The PR #3 source branch and `claude/run3-integration` diverge at `4e5dd35`.
`run5-squad-development` points at `4e5dd35` (no work) and was not involved.

Run 1's 8 commits, Run 2's 2, Run 3's 3 (two merges and the repair/tests commit) and Run 4's 1 are all
reachable from the new branch. Merge commit: `34ddc13`, followed by the consolidation fixes.

## 2. Conflicts

Git reported **no textual conflict** merging Run 4 into Run 3. The files both sides touched were checked
semantically instead:

| Area | Finding |
|---|---|
| 12 locale files | Run 4 added six keys under `endOfSeason`; Runs 1-3 added their own blocks elsewhere. Auto-merged, all valid JSON, `src/i18n` coverage and placeholder-parity tests pass. |
| `db/src/repositories/news_repo.rs` | Only Run 4 touched it. |
| `Game` struct (`game.rs`, `game_persistence.rs`) | Run 3 added no `Game` field the Run 4 summary needs; this branch adds one `#[serde(skip)]` field (see B). Disjoint. |
| Migrations | Run 4 added none. v062/v063 from Run 3 are unchanged, `EXPECTED_MIGRATION_COUNT` still 63, no released migration edited. |
| `end_of_season` | Run 4's `summary.rs` is independent of Run 1-3 code. |

Features preserved: Run 1 (signing bonuses, instalments, sell-on clauses, player consent, stepping-stone
promise, deadline day, inbox answers), Run 2 (separate defensive formation, presets, custom tactics, M12 tactics
integration), Run 3 (v062/v063, Run 2 dev-save repair), Run 4 (lifecycle rules, four-season gate,
`sync_news_list` id-reuse fix, `orderStandings` use, enriched end-of-season summary). The Run 1-3
test suites pass unchanged except for the one test whose pinned behaviour was deliberately changed (B below).

## 3. Migrations

Kept exactly: v061 stable base; `v062_transfer_obligations.sql`; `v063_out_of_possession_formation.sql`
(byte-identical to Run 3).

Tests (all on temporary directories; no personal save, no APFM database was opened):

| Test | What it proves |
|---|---|
| `every_upgrade_path_ends_on_the_schema_of_a_fresh_database` | Released v061, Run 1 dev save, Run 2 dev save (repaired), and the same through the plain entry point each end with the *same tables, columns (name, type, NOT NULL, default, primary key) and indexes* as a database created fresh. Not just `user_version`. |
| `a_v61_upgrade_keeps_every_row_of_every_table` | Every row of every pre-existing table is value-identical after upgrade; `transfer_obligations` starts empty. |
| `the_run2_repair_is_idempotent_down_to_the_bytes` | After the first repair, further opens (`open_save`, `open`) leave the file byte-identical; the backup is still the original. |
| `an_existing_backup_is_never_overwritten_and_the_save_is_still_copied` | **Defect fixed** (§5): see below. |
| `a_failed_backup_leaves_the_run2_save_unrepaired` | If the pre-repair copy cannot be taken, version and schema are unchanged. |
| `saves_missing_what_their_version_promises_are_refused_untouched` | v63 without the column, v63 without the table, v62 claiming more than it holds: refused naming the missing table/column, bytes untouched. |
| existing Run 3 tests | Run 1 dev save, Run 2 dev save, v62 holding both, v61 through both migrations; future version refused. |
| `a_run2_development_career_loads_keeps_its_tactics_and_then_books_obligations` | A **real career file** shaped as the Run 2 dev build wrote it (via `SaveManager`): loads, tactics intact, file repaired, backup byte-equal to the original, a later instalment transfer persists. |
| `a_run1_development_career_keeps_its_obligations_and_gains_the_defensive_shape_column` | Run 1 dev career file: obligations and transfer survive, attacking shape kept, defensive shape reads as none, column added. |
| `a_released_v061_career_loads_and_gains_both_structures` | v061 career file: loads, both structures created, playable on. |
| `a_career_file_that_contradicts_its_version_is_refused_untouched` | Via `SaveManager::load_game`. |

## 4. Remaining issues, fixed

### A. Inbox and news order after a reload
Cause: both repositories loaded `ORDER BY date DESC`, reversing the order the game holds them in and
leaving same-day entries in whatever order SQLite returned. Fix: both load in insertion order
(`ORDER BY rowid`). `replace_messages` rewrites the inbox from the in-memory list, so insertion order is
that list's order. `sync_news_list` is incremental: it appends new ids and updates existing rows in place,
and when a retained id would now come after a new article (the season rollover reuses `season_preview`
behind the new awards article) or two articles swapped, it rewrites the feed in the game's order
(`stored_order_conflicts_with` / `rebuild_news`). Tests: `a_reloaded_inbox_keeps_the_order_the_game_held_it_in`,
`a_second_save_keeps_the_survivors_in_order_and_appends_the_new_mail`, `a_reloaded_feed_keeps_the_order_the_game_held_it_in`,
`an_article_whose_id_is_reused_later_in_the_feed_keeps_the_games_order` (all seen failing first), and the
four-season SQLite test, which no longer sorts inbox/news and compares the whole world **in order** at 17 reloads.
Not changed: archived mail (`archived_messages`) is documented as "newest first" and is read by no ordering-sensitive code.

### B. Loading a save no longer moves matches
Cause: `SaveManager::load_game` ran `calendar_repair::repair_loaded_calendar`, which postpones matches that sit in
international windows or inside a club's rest window, posted a notice and re-saved the file. A save written right
after a rollover therefore came back with 239 matches moved. Fix: the load only sets
`Game::calendar_repair_pending` (`#[serde(skip)]`); the repair runs once, at the start of the first day played from the loaded
state (`calendar_sweep::settle_loaded_calendar`, called from `turn::begin_day` and `process_day`). Legitimate
postponements are unchanged and still happen, before the day's football, so a match dated that day can still move.
Tests: `loading_a_save_does_not_move_a_clashing_match_but_the_first_day_does` (red first: the load moved
a match), the Run 3-era `loading_a_save_repairs_clashing_matches_and_persists_them` rewritten as
`a_saves_clashing_matches_are_repaired_on_the_first_day_and_persisted_by_the_next_save` (it pinned the old
behaviour; every substantive assertion is kept), `a_career_reloaded_just_after_a_rollover_plays_on_like_the_one_never_saved`
(the reloaded and never-saved careers are the same world after 40 days).

### C. `Dashboard.test.tsx > clears a stale active save id…`
Cause: not the component. Under jsdom `window.localStorage` is a `Storage` whose properties are its items, so
`vi.spyOn(window.localStorage, "getItem")` never replaces the method and observes zero calls; the dashboard was
behaving correctly. The method lives on `Storage.prototype`. New helper `src/test-utils/storageSpy.ts` spies where the
method really is (and on the instance when the setup shim backs storage). Assertions unchanged. Mutation check:
removing `setActiveSaveId(null)` in the component makes the test fail; restoring it passes.

## 5. Other defect found while testing

**Run 2 repair backup**: `reconcile_run2_dev_save` skipped the pre-repair copy whenever `<save>.db.pre-v063.bak` already
existed, so an older, unrelated backup stood in for "a copy before modification" and the actual file was repaired with
no copy of its own. It now takes a fresh copy under the first free numbered name and never overwrites.
Introduced by Run 3 (pre-existing relative to this consolidation).

## 6. M12

The real `apfm-match-lab` is **not available** to this session: it is not among the repositories the session's GitHub
credential can read (`add_repo` was refused: not found / no access), and it is absent from the container. It is a path
dependency outside this repository, `UNLICENSED`.

To compile at all, a throw-away stub exposing only the symbols `ofm_core::spatial_runner` imports was placed **outside** the
repository (`/home/apfm-match-lab`). It always returns an error and is not committed; `Cargo.lock` is unchanged and the
`--locked` runs below pass with it. **Nothing run against it validates M12.** The six tests that exercise the simulation fail
*by construction* here and are listed rather than hidden:

- `ofm_core spatial_runner::tests::m12_execution_projects_a_deterministic_coherent_match_report`
- `openfootmanager commands::spatial_match::tests::{a_stale_completed_fixture_leaves_no_partial_m12_finalisation,
  concurrent_m12_finalisation_consumes_one_live_session_once, a_finalized_m12_match_and_replay_survive_a_fresh_sqlite_reload,
  m12_finalisation_before_a_live_session_is_rejected_without_mutation, m12_finalisation_is_single_use_and_rejects_a_second_request}`

Every failure message reads `InputContract("M12 stub: …")`. The other `spatial_*` tests (adapter, projection, output contract,
replay persistence) do not need the engine and pass.

Prepared for the local run: `scripts/apfm/run-m12-tests.sh`. It locates the real engine at the path `ofm_core` expects
(or links `APFM_MATCH_LAB=<checkout>` there), **refuses** to run when that directory is a stub, requires each test filter to
match at least one test (zero tests is not a pass), and runs `spatial_runner`, the `spatial_` tests and
`commands::spatial_match`. It was exercised here only to confirm it refuses the stub (exit 3).

## 7. Tests actually run

Environment: Linux container, 4 cores, Rust 1.95.0 (pinned), Node 22.23. GTK/WebKit dev packages installed with `apt` so the
Tauri root package builds. M12 = local stub (§6), so the six listed failures are expected there and prove nothing either way.

| Check | Command | Result |
|---|---|---|
| Rust, whole workspace | `cargo test --locked --workspace --exclude mcp-e2e --no-fail-fast` | **4068 passed, 6 failed, 33 ignored.** The 6 failures are exactly the M12 tests of §6 |
| Rust, MCP feature | `cargo test --locked -p openfootmanager --lib --features mcp --no-fail-fast` (run before the last test-only edit in `db`, which the root package's tests do not touch) | 619 passed, 5 failed (the five `commands::spatial_match` tests), 1 ignored |
| `db` crate after the final edit | `cargo test --locked -p db` | 400 + 1 + 2 + 7 passed, 0 failed, 1 ignored |
| Season lifecycle (4 seasons, 17 SQLite reloads, extra seeds, live path, post-rollover twin) | part of the workspace run: `season_lifecycle` | 6 passed, 0 failed |
| Clippy, default | `cargo clippy --locked --workspace --exclude mcp-e2e --all-targets -- -D warnings` | clean |
| Clippy, MCP | same with `--features mcp` | clean |
| Format | `cargo fmt --all -- --check`; `npm run format:check` (Biome, 794 files) | clean |
| TypeScript | `npx --no-install tsc --noEmit` (also inside `npm run build`) | clean |
| Frontend build | `npm run build` | passes |
| Biome lint | `npm run lint` | exit 0 |
| Vitest | `npm test` | **311 files, 2749 tests, all passed** (Dashboard included) |
| Translations | `npm exec --no -- vitest run src/i18n` | 9 files, 35 tests passed |
| Dead exports | `npm run knip` | 1 finding, `DISCORD_INVITE_URL`, pre-existing, untouched |
| M12 engine tests | `scripts/apfm/run-m12-tests.sh` | **not run**: refuses (exit 3) because only a stub is present |

Not run: `mcp-e2e`, the FM24-world tests (`APFM_WORLD_JSON`), the desktop application, the review agents.
Tauri commands were exercised through the root package's own `--lib` tests (526 default / 619 with `mcp`, minus the M12 ones).
Before the fixes, on the merged tree alone (merge commit `34ddc13`), the workspace gave 4053 passed / 9 failed: the six M12
tests plus the three order tests of A that had already been added; i.e. the merge itself introduced no failure.


## 8. Failures, by kind

| Failure | Kind |
|---|---|
| 6 M12 tests listed in §6 | Environment limitation (real M12 engine absent). Same six on `claude/run3-integration`. |
| `loading_a_save_repairs_clashing_matches_and_persists_them` | Introduced by this consolidation on purpose (B) — test rewritten, not weakened. |
| A, B and migration tests that failed first | Red-before-green evidence for defects fixed here. |
| `Dashboard.test.tsx` stale save id | Pre-existing (fails identically on `apfm/main`, Run 1, Run 2 and Run 3); fixed in C. |
| `knip`: `DISCORD_INVITE_URL` unused export | Pre-existing on `apfm/main`; untouched. |

No test was skipped, ignored or loosened to obtain a green result.

## 9. Limitations remaining

- The real M12 engine was not compiled or run (§6). Run 2's defensive-shape effect inside an actual M12 match is unverified.
- The desktop application was not launched; frontend coverage is Vitest, `tsc`, Biome and the build.
- The Tauri commands are exercised through the root package's own `--lib` tests (including `--features mcp`), not through a
  running window. `mcp-e2e` was not run.
- Archived mail still loads newest first (§4 A).
- The gate world is 80 clubs in three countries; the 12-league FM24 world (`APFM_WORLD_JSON`) was not played.
- Review agents (`ofm-architecture-reviewer`, `i18n-auditor`, `ui-accessibility-reviewer`, `ofm-test-reviewer`,
  `ofm-dedup-reviewer`) were not run.
- The Run 2 dev-save repair is still in place; remove it when no Run 2 development save is expected to exist.

## 10. Manual actions before merging

1. Run `scripts/apfm/run-m12-tests.sh` on the machine that has `~/apfm-match-lab`; all six M12 tests must pass.
2. Back up real development saves before first opening them with this build (the Run 2 repair copies its own file, but
   that is the only safety net).
3. Play a few days in the real app after loading a save made near a rollover, and confirm the "postponed" notices appear on
   the first day and not on load.
4. Eyeball the end-of-season screen (promotion/relegation/cups) in light and dark themes.
5. Merge with `apfm/main`'s current head if it moved; this branch is based on `4e5dd35`.
