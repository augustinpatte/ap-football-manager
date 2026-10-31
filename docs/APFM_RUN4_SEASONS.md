# APFM Run 4 — season and competition lifecycle

31/10/2026. Branch `run4-season-engine`, pull request towards `apfm/main` (not merged).
Scope: prove, and where it is broken repair, the whole season lifecycle — fixtures, cups,
promotion and relegation, end of season, rollover, new calendar, money/contracts/squads, inbox,
save/reload — over several consecutive seasons.

## 1. Audit result: what already existed

The brief reads like a greenfield build; the repository is not. Run 4 audited first and extended
what was there. Nothing below was rewritten.

| Brief item | Where it already lives | Verdict |
|---|---|---|
| 1. Fixtures, scheduling, postponements, standings, tie-breakers | `ofm_core/schedule.rs`, `calendar_reconcile/` (postponements: `Fixture::postponement`, `RescheduleReason`), `domain/table_order.rs` (authored tie-breakers incl. head-to-head) | Complete. |
| 2. Cups, draws, progression, finals | `schedule.rs` cup generation, `turn/cup_renewal.rs`, `group_stage.rs`, `competition_completion.rs` | Complete. |
| 3. Promotion / relegation | `end_of_season/berths/` (pyramid and berth rules), `ladder_rules.rs`, `world/ladder.rs` | Complete. |
| 4. End of season, history, rollover | `end_of_season/mod.rs`, `edition_archive.rs` (immutable per-edition archive), `TeamSeasonRecord` | Complete. |
| 5. New-season calendar | `schedule.rs`, `calendar_reconcile::settle_generated_calendar` | Complete. |
| 6. Finance, contracts, squads | `finances/`, `ai_contracts.rs`, `aging.rs`, `youth_intake.rs`, `squad_floor.rs` | Complete. |
| 7. Inbox + frontend milestones | `inbox.rs`, `end_of_season` messages, `EndOfSeasonScreen.tsx` | Messages existed; the screen did not report promotion/relegation or cups. **Gap.** |
| 8. Save/reload | `db/` SQLite repositories, `calendar_repair` on load | Reload tested mid-season only, in JSON for the multi-season case. **Gap** (a persistence bug was found, §3.1). |
| 9. M12 | `ofm_core/spatial_*`, `src-tauri/src/commands/spatial_match.rs` | Untouched, see §6. |
| 10. Multi-season regression | `season-harness` (`gate.rs`: 2 seasons; `nightly.rs`: ignored) | Two seasons in the PR gate, SQLite reload never exercised across rollovers. **Gap.** |

So the work was to measure the engine over **four** seasons with **SQLite** saves at the critical
points, and to repair what that exposed.

## 2. What was implemented

### 2.1 `season-harness`: lifecycle rules (`src/lifecycle.rs`, new)
An observer that attaches to `run_seasons` / `play_day` / `roll_season_over` beside the existing
`Invariants`. It reads game rules from `ofm_core` and restates none of them.

Every day:
- `fixture-ids-unique` — no fixture id twice in a competition.
- `league-pairings-not-duplicated` — an ordered pairing is never scheduled more often than the
  table's legs allow (no duplicated fixture).
- `standings-match-results` — every table row equals the sum of the completed league results that
  name the club (a result is applied exactly once: not twice by a live and a batch path, not lost
  by a reload). The existing `standings-are-consistent` only checked the rows against themselves.
- `postponements-move-forward` — a postponed match sits strictly after its original date.
- `cup-champions-won-their-final` — the champion on an archived knockout edition is the side that
  won the single-match final, shootout included.

At every rollover:
- `history-records-follow-seasons` — each club of a finished table gains exactly one season record,
  stamped with that season; no club holds two for one season.
- `edition-archive-keeps-the-champion` — every finished table is archived, once, with the champion
  the table crowned.
- `milestone-mail-sent-once` — `season_end_N` and `new_season_N+1` are emitted and appear at most
  once; no inbox id repeats.
- `ai-contracts-outlive-the-rollover` — no AI-club player carries a contract that ended before
  today.

### 2.2 End-of-season summary (`ofm_core/src/end_of_season/summary.rs`, new)
`EndOfSeasonSummary` moved out of the 2,100-line `end_of_season/mod.rs` and gained, all
`#[serde(default)]`:
- `user_division_move` (`Stayed | Promoted | Relegated`), `user_next_league_id`,
  `user_next_league_name`;
- `user_cup_results`: each domestic cup the club entered, with its champion and whether the user
  won it, read from the immutable edition archive.

The direction logic (a berth move counts as a promotion even at equal rank) was extracted from
`notify_user_division_change` into `division_change_of`, so the inbox message and the summary
share one rule. Behaviour of the message is unchanged.

### 2.3 Frontend
- `EndOfSeasonScreen`: the last step now states the promotion / relegation / stay and lists the cup
  results (`SeasonMilestones`). Six new keys `endOfSeason.{promoted,relegated,stayedUp,cupResults,cupWon,cupLost}`
  in all 12 locales.
- `EndOfSeasonScreen` and `HomeTab.helpers::getStandingPosition` re-sorted the table with their own
  points / goal-difference / goals formula, which contradicts the champion, promotions and
  relegations the backend decides with authored tie-breakers (head-to-head first in some
  countries). Both now use `lib/standings.ts::orderStandings`, which keeps the backend's order.

## 3. Defects found and fixed

### 3.1 Season preview of the previous season came back after a reload (persistence)
`news_repo::sync_news_list` skipped any article whose id was already stored with the same read
flag. The season preview is always `season_preview`, so from season 2 on the new preview was never
written: the file kept last season's (date, favourite and dark horse of a year earlier) and a reload
showed it. Found by `a_career_survives_a_sqlite_reload_at_every_stage_of_four_seasons` at
"season 2 just rolled over". Fix: an id is unchanged only if date, text and i18n payload also
match; otherwise the row is rewritten. Test: `an_article_reusing_an_id_replaces_the_stored_one`
(seen failing — 0 rows written instead of 1 — before the fix).

### 3.2 Two UIs ranked tables with a formula the backend does not use (frontend, §2.3)
Tests seen failing before the fix: `shows the backend's finishing order when the competition
authors its tiebreakers`, `reads the opponent's position from the backend's order when tiebreakers
are authored`.

## 4. Behaviours observed and deliberately left unchanged
- **A load moves matches that sit in international windows.** `calendar_repair::repair_loaded_calendar`
  settles every unplayed clash at load and posts a "postponed on load" notice; the live game does
  the same one day at a time as windows come into view. Right after a rollover 239 matches of the
  generated world (World Cup June 2034, September windows) are moved by a reload that a live day
  would have moved later. The reload test therefore compares the reloaded world with the saved
  world *after* that same repair, so it asserts that a reload can only postpone, never lose or
  invent a fixture.
- **Order of inbox and news after a load.** The save lists them newest first; memory keeps them
  oldest first; screens sort both. The reload test compares them as sets.
- `Game::squad_floor_top_ups` is a session diagnostic and is never saved (documented on the field).

## 5. Tests

New, all in the PR gate unless noted:

| Test | File | Covers |
|---|---|---|
| `four_consecutive_seasons_hold_every_lifecycle_rule` | `season-harness/tests/season_lifecycle.rs` | 4 seasons, all invariants + all lifecycle rules, an archived edition each season, one history record per season |
| `a_career_survives_a_sqlite_reload_at_every_stage_of_four_seasons` | same | SQLite save + fresh-manager load at opening, day 15, day 100, last matchday, just after rollover, in each of 4 seasons (17 reloads); each reload must equal the saved world |
| `a_reloaded_career_plays_on_exactly_like_the_one_never_saved` | same | save at day 60, play 30 more days on both copies: identical worlds |
| `the_lifecycle_rules_hold_for_other_seeds_and_the_live_path` | same | seeds 2, 3 (batch) and 4 (player's match through the live path), 2 seasons each |
| `each_lifecycle_rule_fails_on_the_fault_it_names` | same | each rule is handed a deliberately broken world and must report it (and must be silent on the intact one) |
| `the_summary_*` (5), `a_summary_without_the_new_fields_still_loads` | `ofm_core/tests/end_of_season_tests.rs` | promoted / relegated / stayed, cups entered and won / lost, old summary JSON loads. Seen failing with the summary fill disabled (5 of 5) |
| `an_article_reusing_an_id_replaces_the_stored_one` | `db/src/repositories/news_repo.rs` | §3.1 |
| 4 new Vitest cases | `EndOfSeasonScreen.test.tsx`, `HomeTab.helpers.test.ts` | §2.3 |

## 6. M12

The standalone M12 repository (`~/apfm-match-lab`, a path dependency of `ofm_core`) is not
available in the cloud environment this run executed in, and was not touched. No file of the
integration (`spatial_*`, `commands/spatial_match.rs`) was edited. The season rules above read
completed fixtures and their stored results, which M12 finalisation writes through the same
`post_match` path as the engine, so M12 matches enter the lifecycle by the fixture result alone.
To compile here, a throw-away stub crate exposing only the symbols `spatial_runner.rs` imports was
placed **outside** the repository at `/home/apfm-match-lab`; it is not committed, `Cargo.lock` was
restored to the committed content, and the live-path tests drive `ofm_core::matchday` (the 2D
engine path), not M12 simulation. Anything that needs real M12 output was therefore **not run**.

## 7. Exact results

Environment: Linux container, 4 cores, Rust 1.95.0, Node 22. The M12 crate was a local stub (§6),
so every M12-dependent test fails by construction; they are listed, not hidden.

| Check | Command | Result |
|---|---|---|
| Season gate (existing, unchanged) | `cargo test -p season-harness --test gate` | 12 passed, 0 failed, 17.4 s |
| New lifecycle tests | `cargo test -p season-harness --test season_lifecycle` | 5 passed, 0 failed, 65 s with the rest of the suite running (57 s when run alone, before the extra-seed test was added) |
| Rust workspace (library crates) | `cargo test --workspace --exclude openfootmanager --exclude mcp-e2e --no-fail-fast` | **3453 passed, 1 failed, 31 ignored**. The one failure is `ofm_core spatial_runner::tests::m12_execution_projects_a_deterministic_coherent_match_report`: `InputContract("M12 stub: ...")` |
| Tauri root package | `cargo test -p openfootmanager --lib` | **526 passed, 5 failed, 1 ignored**. The 5 are all `commands::spatial_match::tests::*` and all die on `InputContract("M12 stub: ...")` |
| Root package compile | `cargo check -p openfootmanager --all-targets` | clean |
| Clippy | `cargo clippy --workspace --exclude openfootmanager --exclude mcp-e2e --all-targets -- -D warnings` | clean |
| Format | `cargo fmt --all` (applied), `npm run format:check` | clean |
| Frontend tests | `npm test` | 2721 passed, **1 failed**, 308/309 files. The failure, `Dashboard.test.tsx > clears a stale active save id when a later refresh cannot load it`, fails identically on the untouched base commit (checked with `git stash -u`): **pre-existing, not fixed here** |
| Locale gates | `npm exec --no -- vitest run src/i18n` | 9 files, 35 tests passed |
| TypeScript + bundle | `npm run build` | passed |
| Lint | `npm run lint` | exit 0 (142 infos, none in touched files) |
| Dead exports | `npm run knip` | 1 finding, `DISCORD_INVITE_URL` — present on the base commit, not touched |

Not run: `cargo clippy --features mcp` and the `mcp-e2e` suite; the three reviewer agents the
repository asks for before a PR (`ofm-architecture-reviewer`, `i18n-auditor`,
`ui-accessibility-reviewer`, plus `ofm-test-reviewer` and `ofm-dedup-reviewer`) — they were not
spawned in this run, so there is no review evidence beyond the tests above; the desktop app was
not launched.

What the four-season run measured (gate world, seed 1, 80 clubs, ENG/PT/AR): all 4 seasons complete,
0 invariant violations, 0 lifecycle violations, 17 SQLite reloads all identical to the saved world
(after the calendar repair of §4), and an archived edition set for each of seasons 2033 to 2036.


## 8. Remaining limitations
- Not run: the Tauri root package (`openfootmanager`, `cargo test --lib`, `--features mcp` clippy)
  and `mcp-e2e` — see §7 for what could be built. No manual play of the desktop app.
- The FM24-world tests (`APFM_WORLD_JSON`) return early without the owner's world file; not run.
- The reload comparison treats inbox/news order as unordered and ignores the squad-floor
  diagnostic (§4); a bug that only reorders messages would not be seen.
- Domestic cup editions are checked through the archive (champion, final, one edition once); a cup
  renewal that never reaches the archive (an unarchivable legacy edition) is not flagged.
- Lifecycle rules are outcome-independent by design: they do not say a given season's table is
  *plausible*, only that it is internally and historically consistent.
- The gate world has 80 clubs in three countries; the 12-league FM24 world was not played.
- The new test file adds about one minute of wall time to the `backend` job (§7).
