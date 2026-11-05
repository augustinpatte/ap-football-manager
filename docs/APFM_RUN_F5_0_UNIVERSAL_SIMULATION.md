# APFM Run F5.0 — Universal match simulation, career integrity and storage efficiency

Branches: OFM `claude/run-f5-0-universal-simulation`, M12 `f5.0/universal-simulation` (see §21).
Started from the verified F4.4 heads (OFM `67c67a77`, M12 `d4e6f32`). Nothing is merged into main.
Date of the run: 2026-11-04.

**Verdict (§23): see the end. Short form: GO for F5.1 on the career-integrity and storage goals; the F4.4
football defects are untouched and still block calling the football final.**

All numbers below were measured in this run, on generated careers (England + Portugal + Argentina,
80 clubs, ~2,000 club fixtures a season, every competition played, release build, Apple silicon laptop),
and every claim names the test or study that measured it. A check that was not run is listed in §18/§19
as not run; none is described as passing.

---

## 1. Existing functionality audit

Done before any change, from the code paths and from two reproducible studies
(`crates/season-harness/tests/f50_consequences.rs`, `f50_storage.rs`). Classification:
**V** implemented and verified · **I** implemented but incomplete · **U** implemented but untested · **M** missing.

| Feature | Where it lives | Before F5.0 | After |
|---|---|---|---|
| Interactive M12 finalization | `application::interactive_match::finish_interactive_match_internal` → `live_match::finish_spatial_match` → `finish_taken_live_match` | **V** (identity check, `Scheduled` check, retry keeps the match, reload-safe; F4.2 tests) | V |
| Instant user match | `live_match::start_live_match_with_identity(.., "instant")` → `finish_live_match` (runs the remainder) | **V** | V |
| User match skipped by the calendar | `turn::process_day` / `matchday::play_user_matchday_with_capture` / `fast_forward` | **V** (fast-forward ≡ day-by-day, `tests/fast_forward.rs`, `f3_fast_forward.rs`) | V |
| AI v AI, active competitions | `live_match_manager::play_unwatched_fixture_with` → `post_match::apply_match_report_in` | **V** for scoring, appearances, minutes, cards, injuries, bans, wear (this run's study: 0 banned and 0 injured players with minutes in ~104k player lines) | V |
| AI v AI, **dormant** competitions (outside the followed scope: most of the world in a default career) | `turn::dormant` | **I** — scoreline, standings, appearances and 90 minutes only. No scorers, no assists, no cards, no injuries, no bans, no fatigue | **V** (§5) |
| Cup matches | same pipeline, extra time and shootout by `fixture_allows_extra_time` | **V** | V |
| Player season statistics | `post_match::apply_player_stats` | **V** (reconciles to the match records: 0 faults over 2 seasons × 3,856 matches) | V |
| Team statistics / standings / form | `apply_match_report_in`, `StandingEntry::record_result` | **V** | V |
| Fatigue and recovery | `player_wear`, `training::process_training` | **V** for active clubs; **I** for dormant (no charge) | V |
| Injuries | `turn::match_injuries` | **V** active; **I** dormant | V |
| Red cards and second yellows → suspensions | `suspension::settle` | **V** | V |
| Yellow-card accumulation | — | **M** (a player could collect 27 yellows and never miss a match) | **V** (§7) |
| Eligibility of suspended/injured players (AI and user) | `match_day_eligibility::eligible_among`, `Player::can_play` | **V** (study + new gate rule) | V |
| AI squad rotation for fatigue | `turn::squad::ai_select_starting_xi` | **V**, graded by manager quality (see §8 for the measured residue) | V |
| Idempotent finalization | `finish_taken_live_match` checks `status == Scheduled`; **`apply_match_report` itself did not** | **I** — a second direct call re-applied appearances, cards, bans and the table | **V** (§4) |
| Calendar processing exactly once | `calendar_sweep`, `matchday::stranded_fixtures`, harness `no-stranded-fixture` | **V** | V |
| Match history in the save | fixtures' `MatchResult.report` + `player_match_stats`/`team_match_stats` | **I** — complete but ~22 KB a match on disk and unbounded in memory | **V** (§11) |
| Season statistics | `Player.stats`, reset at the rollover | **V** | V |
| Save serialization / DB | SQLite per save, `crates/db` | **V**; migrations v066, v067 added | V |
| Replays | `match_replays` | **I** — 96 MB of JSON per stored replay, never pruned | **V** (§11) |

## 2. Architecture before and after

Before: two finalization entry points converge on `turn::post_match::apply_match_report_in`
(statistics → promises → wear → injuries → bans → morale → form → board → news). The dormant tier
bypassed it entirely.

After:

```
engine / M12 / OFM live engine ─ MatchReport ─┐
                                              ├─► apply_match_report_in ─► apply_player_consequences ─┐
dormant lightweight tier ─ lightweight_report ┘        (full: standings, morale, form, news)         │
          └─────────────── (scoreline result, scorers) ─► apply_player_consequences ◄────────────────┘
apply_player_consequences = statistics → condition → injuries → bans   (ONE function)
```

`turn::post_match::apply_player_consequences` is the single stage every club fixture ends in. The
lightweight tier produces the same `engine::MatchReport` type the full engines produce (it is built
under `ofm_core/src/turn/`, the one place the engine bridge is allowed to live; `engine` still imports
nothing from `domain`).

## 3. Match-result contract

The canonical contract is the existing `engine::MatchReport` (score, goals with scorer/assister/source,
events with minute/side/player/secondary/detail, per-player `PlayerMatchStats` including
`physical_load`, team stats, possession, penalties). It is what the interactive M12 path, the instant M12
path, the OFM live engine and now the lightweight tier all hand to finalization.
I did **not** introduce a parallel versioned struct: a second type would have been a second place for the
two to drift, and the persistent forms are already versioned (SQL migrations; the interactive checkpoint
has its own `schema_version`). What the contract guarantees, and where each guarantee is enforced:

| Guarantee | Enforcement |
|---|---|
| Stable identity (fixture id, competition id, season) | `FinalizationContext` identity check in `finish_taken_live_match`; the stats lines carry `fixture_id` |
| One finalization per fixture | status guard in `apply_match_report_in` (new) + `Scheduled` check in the application layer |
| Goals ↔ scorers ↔ player totals | lightweight tier: scorers are drawn *from* the score; study: credited goals = score |
| Cards, second yellows, reds → bans | `suspension::settle` reads the report's events only |
| Injury events → injuries | `turn::match_injuries` |
| Minutes and load → condition | `player_wear::apply_match_wear_with_load` |
| No invented events | the lightweight model never alters a score; the score is its input (§5) |

Fields the lightweight tier does not produce, by design: substitutions, own goals, penalties, possession,
shot and pass counts. They are absent from its report, not fabricated (§5, §19).

## 4. Finalization pipeline

`apply_match_report_in` now refuses a fixture that is missing or already `Completed` (logged, nothing
applied). The application layer's check is unchanged. Failure handling is unchanged and was already
sound: every validation in `finish_taken_live_match` happens before the first mutation, a failed
finalization puts the finished match back so the player can retry
(`a_failed_finalisation_keeps_the_finished_match_for_a_retry`), and the whole day is one `update_game`
closure under the game lock.

Tests: `post_match::tests::a_fixture_is_finalised_once` (appearances, yellows, ban, table all of one
match after two applications). Mutation: removing the guard makes it fail (§17).

## 5. AI-club consequences — the lightweight tier

`turn/dormant_report.rs` (new) and `turn/dormant.rs`:

* the eleven: best eligible by rating, with a tired player ranked down by `0.5 × (70 − condition)` so a
  worn-out first choice is rested for a nearly-as-good fresher player (tests);
* goals: the drawn score's goals are given to scorers by position and shooting, 70% assisted;
* cards: per-player yellow rate `0.10 × (0.5 + aggression)`, 3% of those a second yellow, 0.35% straight reds;
* injuries: per-player `match_injury_probability` (the same base risk the engine path rolls from),
  applied by the engine path's own `apply_match_injuries`;
* everything then goes through `apply_player_consequences`.

Calibration against the full engine, same seeds, two seasons (`f50_every_fixture_leaves_consequences`):

| per team-match | full engine (active) | lightweight (dormant) |
|---|---|---|
| yellow cards | 1.13 – 1.20 | 1.25 |
| red cards | 0.039 – 0.051 | 0.034 |
| injuries | 0.214 – 0.235 | 0.251 |
| assists per goal | 0.92 / 0.82 | 0.70 |
| goals | 1.65 – 1.96 | 1.25 (the pre-existing scoreline model; the engine's excess is the F4.4 debt, §20) |

Dormant clubs now also count as "playing" for the training ground (`training::teams_playing_on`,
`tapering_teams`): they no longer train on a matchday and they taper before a congested week.
One existing test asserted the old rule and was rewritten to assert the new one
(`a_two_match_week_in_a_dormant_competition_is_recovery_work_like_an_active_one`).

The player match *lines* are not produced for dormant fixtures: that is the storage trade (§11).
Their goals, assists, cards, minutes, injuries and bans all reach the players' season statistics.

## 6. Calendar advancement

Already verified by `fast_forward.rs` / `f3_fast_forward.rs` (a fast-forward and the same days played one
at a time end in byte-identical worlds, a postponed fixture is played once, a save taken part-way carries
on identically, rollover inside a run) and by the harness rule `no-stranded-fixture`. F5.0 re-ran them
after the dormant change (they play the default `Followed` scope, so they exercise it); the final tree
passes them (§23, criterion G). New: `a_season_of_full_matches_keeps_availability_and_reconciles` and the
`banned-player-never-plays` harness rule, which runs in the PR gate for every generated career.

Not separately re-measured in F5.0: "advance one day", "seven days", "thirty days" as application
commands (they are the same `fast_forward::run` the tests above cover), "through a transfer window"
(`fast_forward.rs` has the deadline tests).

## 7. Disciplinary system

`suspension.rs`: straight red 3 matches, second yellow 1 (unchanged), and **new**: every 5th yellow card
of a season (any competition) is a 1-match ban. Tests: `the_fifth_yellow_card_of_a_season_costs_one_match`,
`a_yellow_card_between_the_thresholds_costs_nothing`. The count is the season's `stats.yellow_cards`, so
it resets at the rollover. Bans began, 2 seasons, 80 clubs: 1,086 → 1,987 (user club 22).

## 8. Injuries and fatigue

Unchanged for active clubs. Measured: AI starters (45+ minutes) kick off with mean condition 86.6
(3.7% below 50) against 96.5 (0.0%) for the user's club, which is the strongest club of its country with a
deep squad; AI clubs are therefore *more* fatigued, not exempt. The residue is a finding for F5.1: AI
rotation is threshold-based (`ai_select_starting_xi`) and still starts ~1–4% of players under 50.

## 9. Individual and team statistics

Season totals reconcile to the match records: appearances, minutes, goals and yellow cards of every
player, 2 seasons, 0 faults. (An appearance is credited to a substitute who came on at the final whistle:
173 of 104,027 lines have 0 minutes; that is football's own rule, not a defect.)

## 10. Cross-mode consistency

`application::interactive_match::tests::every_simulation_mode_updates_the_same_career_systems`: the same
fixture finalised as an interactive M12 match, as an instant M12 match and as an OFM-engine instant match
leaves the same nine categories of consequence (fixture settled, both table rows, appearances with minutes,
lines captured, condition spent, goals within the score, second finish refused). Scores differ by engine,
by design. Interactive → instant completion is covered by
`a_managed_match_is_finalised_once_and_survives_a_reload` and `two_simultaneous_finishes_apply_the_match_once`.
The F4.4 scoring discrepancy (M12 ≈ 4–5 goals a match at equal strength, the statistical tier ≈ 2.5) is
**not** hidden and **not** touched: it is §20.

## 11. Storage architecture

Five separated stores, none redundant with another:

| Store | Holds | Bound |
|---|---|---|
| A. live checkpoint | `interactive_match_checkpoint`, one singleton row, no frames, < 400 KB (F4.1) | one |
| B. completed-match summary | `fixtures.result` (score, scorers, compact events) in `competitions`; in `competition_edition_archive` after the season | per fixture, deflated once archived |
| C. current state | players, teams (conditions, bans, injuries) | world size |
| D. aggregated statistics | `Player.stats` (season), `player_stat_totals` / `team_stat_totals` (folded seasons) | per player / team |
| E. optional replay | `match_replays` | **5 most recent, deflated** |

Changes (migrations v066, v067; both forward-only, behind the existing verified pre-migration backup
`GameDatabase::open` already takes of any save below the current version):

1. **v066 compact match history.** `player_match_stats`/`team_match_stats` repeated six 36-character ids,
   the date, the competition and the score on every one of ~27 rows a match, and two indexes repeated
   the keys again (~450 bytes a row). Now: `stat_ids` (each id once), `match_stat_fixtures` (a match's
   facts once), and line tables of small integers (~43 bytes a row). Existing history is migrated
   record for record (`a_v65_upgrade_carries_the_match_history_over_record_for_record`, which includes a
   fixture only a team line knows and a record that disagrees with its match).
2. **Retention of lines.** Per-match lines are kept for 5 seasons (`SEASONS_OF_LINES_KEPT`), and always
   the latest 5 of each player/team; older lines are folded into per-player and per-team totals. The
   statistics overviews read exactly those sums, so what a screen shows does not change
   (`folded_seasons_change_nothing_a_screen_shows`, which compares four screens' DTOs between a
   fully-kept and a folded history). The player's *recent matches* list is never older than the window.
   Why: a line costs ~500 bytes of memory; thirty seasons kept were 800 MB of heap.
3. **v067 deflate** of the edition archive (a finished edition's whole fixture list; 14% of its JSON,
   measured) and of replays (6% of its JSON, measured: 96 MB → 5.6 MB). Old rows stay JSON and still load.
   The archive writer also stopped re-serializing every edition on every save.
4. **Replay retention**: 5 most recent. Nothing but re-watching needs a replay; the match is complete in B.

Not stored, ever: positions per tick (except the optional, bounded replay), ball tracks, decision trees,
intermediate tactics, debug traces. The `save-snapshots` feature (20 full copies of the save) is
off by default and is a developer feature; it is noted here because with it on, every figure below is ×21.

## 12. Storage benchmarks

Real files in the saves directory (database, rollback journal, index; the database is not in WAL mode, so no
WAL file exists), measured after saving, with the same career before and after the changes.
"History" = lines, ids, match facts, archived editions, competitions' fixtures.
`F50_SEASONS=30 cargo test --release -p season-harness --test f50_storage f50_storage_growth -- --ignored --nocapture`

| Point | File before | File after | History after (Δ since previous row) |
|---|---|---|---|
| baseline (new career) | 6.3 MB | 6.3 MB | 1.0 MB |
| after ~1 match-day (10 fixtures) | 6.55 MB | 6.47 MB | +57 KB |
| ~100 fixtures | 8.49 MB | 7.53 MB | +541 KB |
| ~380 fixtures | 14.95 MB | 10.59 MB | +1.48 MB |
| 1 season | 42.4 MB | 14.5 MB | +1.50 MB (partial season) |
| 3 seasons | **118.5 MB** | **30.9 MB** | 12.1 MB |
| 5 seasons | – | 44.6 MB | 16.9 MB |
| 10 seasons | – | 69.2 MB | 21.9 MB |
| 20 seasons | – | 120.6 MB | 31.3 MB |
| 30 seasons | (≈ 1.1 GB extrapolated) | **168.2 MB** | **40.0 MB** |

Bytes per fixture, match history only: **~22 KB before**; **1.9 KB** while the lines are still being
kept (first 5 seasons, ≈ 3.8 MB a season); **0.46 KB** in steady state (≈ 0.92 MB a season for ~2,000
fixtures, because folded seasons stop growing the lines table). A 380-match league season costs ≈ 0.17 MB
in steady state and 0.72 MB while its lines are kept: under the ~1 MB target in both.

What is *not* match history and keeps growing at ≈ 3.8 MB a season (outside this mission, reported so it
is not lost): `retired_players` 35 MB, `cash_journal` and its three indexes 58 MB, `retired_managers`
10 MB and `messages` 7 MB at season 30. `cash_journal` alone is a third of a 30-season file.

Replays: one M12 replay is 96.5 MB as JSON, 5.6 MB deflated. Bounded at 5: ≤ 28 MB.

## 13. One-season validation

`f50_every_fixture_leaves_consequences` (every competition played, 2 seasons, seed 11; first season shown):

* every completed club fixture produced exactly one capture; 0 fixtures captured twice;
* season totals = captures for every player (appearances, minutes, goals, yellows): **0 faults**;
* players with minutes while suspended: **0**; while injured: **0** (of 104,027 lines);
* standings are consistent (existing harness rule, every day).

International fixtures (248 in a World Cup qualifying season) and the 220 league fixtures an Argentine
split season has already played before the career begins carry no capture, by design.

## 14. Multi-season validation

`f50_storage_growth` plays 30 consecutive seasons with a save and a reload (asserting the loaded stats
and fixture counts equal memory) at each reporting point, and the existing harness gates
(`gate.rs`, `season_lifecycle.rs`) play consecutive seasons under the full invariant set including the
new `banned-player-never-plays` rule. Not done: manager-dismissal continuity (the harness keeps the
manager employed on purpose, `RunOptions::keep_manager_employed`; its dismissal test is
`a_manager_the_board_would_sack_is_reported_when_nothing_holds_them_on` in `gate.rs`, which ran).

## 15. Performance

Release build, 80-club world, every competition played (the heaviest case):

| | Result |
|---|---|
| one season (~2,000 fixtures) | 1.7 s (season 1) → 3.2 s (season 3) → 3.0–4.5 s (seasons 10–30), saves included |
| one fixture, full engine | ≈ 1 ms |
| save | 39 ms (new) → 158 ms (1 season) → 931 ms (10 seasons) → 1.5 s (30 seasons) |
| load | 62 ms → 113 ms → 449 ms (10) → 1.0 s (30) |
| whole-game heap (a clone's size) | 84 MB at 30 seasons |
| match statistics heap | 46 MB at 30 seasons (136 MB at 5 seasons *before* retention extrapolated to ≈ 800 MB at 30) |
| resident set of the benchmark process | 1.5 GB at 30 seasons (includes the harness's own copies; the game plus statistics are ≈ 130 MB of it) |
| dormant fixture, the consequence stage | +2.5 µs on a 10.7 µs scoreline-only fixture (13.2 µs, game clone included; `dormant_fixture_cost`), ~1/400th of a full-engine fixture |

Bottleneck found: the save rewrites the whole game every time (≈ 1.5 s at 30 seasons, dominated by the
non-match tables above). A second: `persist_new_editions` serialized the entire archive on every save.
It is fixed. Per-day application latency was not separately profiled.

## 16. Save compatibility

* v066 and v067 are forward-only; a save below the current version gets the existing verified
  `*.db.pre-v063.bak` backup first (the one-time cost is a second copy of the old file on disk until the
  player deletes it — worth saying to a player with a large save).
* Tested: v65 → v66 history carried over record for record; legacy JSON archive rows and legacy JSON replays
  still load; the three "released build" imitation tests (v061, Run 1/Run 2 development saves) were
  updated to drop the new structures and still load.
* A completed or in-progress interactive match in a saved career: unchanged paths, covered by
  `a_match_saved_in_progress_resumes_and_finishes_once`.
* Old saves keep growing until their first fold: the first match played after loading a long career folds
  seasons beyond the window; the next save rewrites the stats tables once (≈ 1–2 s). The freed pages are
  reused by later growth; the file does not shrink without a `VACUUM`, which is deliberately not run
  automatically (it needs free space equal to the file).
* Older builds cannot open a v066+ save (`IncompatibleVersion`), as for every migration.

## 17. Regression tests

New and changed, all passing at the time of writing unless §18 says otherwise:

| Test | Scenario |
|---|---|
| `post_match::a_fixture_is_finalised_once` | replay of a report changes nothing |
| `post_match::the_fifth_yellow_card_of_a_season_costs_one_match`, `a_yellow_card_between_the_thresholds_costs_nothing` | accumulation, with its control |
| `dormant::every_goal_of_a_dormant_score_belongs_to_a_player` | scorers = score, assists ≤ goals (20 days) |
| `dormant::a_ban_is_served_in_a_dormant_match_the_banned_player_sits_out` | bans count down and the banned do not play |
| `dormant::a_dormant_match_tires_the_players_who_played_it` | wear, bench untouched |
| `dormant::a_worn_out_first_choice_is_rested_for_a_nearly_as_good_replacement` | rotation and its limit |
| `dormant::a_dormant_day_is_deterministic` | same seed, same everything |
| `stats::tests::*` (domain) | fold preserves every sum; latest lines kept; no fold within a season; idempotent |
| `stats_repo::folded_totals_survive_save_and_load`, `a_fold_makes_the_next_save_a_full_rewrite`, `saving_one_more_match_writes_only_that_match` | persistence of the fold, append-only writes |
| `game_database::a_v65_upgrade_carries_the_match_history_over_record_for_record` | migration |
| `edition_archive_repo::*`, `replay_repo::*` | deflate round trip, legacy rows, damaged blob, replay retention |
| `commands::stats::folded_seasons_change_nothing_a_screen_shows` | four screens, folded vs kept |
| `interactive_match::every_simulation_mode_updates_the_same_career_systems` | three modes, one career |
| `f50_consequences::a_banned_player_with_minutes_in_a_match_is_reported`, `a_season_of_full_matches_keeps_availability_and_reconciles` | the gate can fail; a season holds (0 reconciliation faults, bans handed out, matches captured) |
| `dormant_report::a_lightweight_report_is_consistent_and_draws_cards_and_injuries`, `goals_go_to_the_players_whose_position_scores_them` | 400 reports: every event on the right side, lines agree with events, goals = score, no self-assist; position weighting |
| `dormant::dormant_cards_and_knocks_become_bans_and_injuries_on_players`, `a_club_playing_twice_in_a_day_fields_only_who_can_still_play` | events become real bans and injuries; a second match the same day |
| `post_match::a_second_yellow_that_crosses_the_threshold_is_one_ban`, `yellow_cards_do_not_accumulate_across_a_rollover` | the edges of accumulation |
| `stats_repo::every_kind_of_competition_survives_the_match_history`, `totals_of_a_later_epoch_are_written_even_when_the_lines_did_not_move` | all 8 competition kinds and every totals column through a second reader |
| `replay_repo::a_damaged_replay_fails_with_a_translated_error` | translated error on a damaged replay |

**Mutation testing** (by hand, one mutant at a time in the working tree, restored and `touch`ed after each;
ten mutants of the critical finalization, eligibility, retention and persistence logic):

| Mutant | Killed by |
|---|---|
| finalization guard removed | `a_fixture_is_finalised_once` |
| yellow threshold fires on every booking | `a_yellow_card_between_the_thresholds_costs_nothing` |
| yellow threshold never fires | `the_fifth_yellow_card_of_a_season_costs_one_match` |
| bans never served | `a_straight_red_card_costs_three_matches` |
| dormant matches have no consequences | `a_dormant_match_tires_the_players_who_played_it` |
| dormant eleven ignores eligibility | `a_ban_is_served_in_a_dormant_match_the_banned_player_sits_out` |
| fold drops the totals | `folding_old_seasons_changes_no_sum_an_overview_reads` |
| totals epoch ignored on save | `totals_of_a_later_epoch_are_written_even_when_the_lines_did_not_move` |
| dormant scorers not credited | `every_goal_of_a_dormant_score_belongs_to_a_player` |
| replay retention changed | `only_the_most_recent_replays_are_kept` |

On the first pass three mutants **survived** and each exposed a weak test that was then fixed: the epoch
mutant (the append path's own checks hid it: a test now changes only the totals), the retention mutant (the
test used the constant it was testing: it now counts rows) and the dormant-consequences mutant (the wear
test passed vacuously when nobody had played: it now requires the eleven to have minutes). Three more
mutants did not apply or compile (`cargo fmt` had reflowed the lines they targeted) and were re-written;
all ten are killed in the final tree. The test reviewer (§18) then found eight more mutants that survived
(cards and injuries never drawn; scorers from the wrong club; a player assisting himself; position ignored for
scoring; the competition kind not persisted; totals columns swapped; a dropped totals column); tests were
added for each and **all eight are killed** (`R1`–`R8`). Final count: 18 mutants, 18 killed.

## 18. Reviewer findings

Four read-only reviewers ran on `git diff 67c67a77..HEAD` (at `ef88c8eb`; the test reviewer re-ran its mutants
on `0501ca03`). `ui-accessibility-reviewer` was **not** run: no frontend file changed.

| Reviewer | Findings | Disposition |
|---|---|---|
| `i18n-auditor` | one: `replay_repo` returned library error text (`error.to_string()`) from the path `load_spatial_replay` returns to the UI | **fixed**: `be.error.gamePersistence.*` keys (existing), test added. No other string added or changed; no locale file needed. |
| `ofm-architecture-reviewer` | (1) a club with two dormant fixtures in one day would field the same eleven twice | **fixed** (`eleven_still_fit`; the calendar's rest rule makes it unreachable in generated calendars, but the defence is cheap). Helper-level test only; the two-fixture wiring is not driven end to end. |
| | (2) a new `#[expect(too_many_arguments)]` whose reason was not a constraint | **fixed**: a `RecordFacts` struct; no new suppression |
| | (3) `simulate_dormant_day` 111 lines | **fixed**: split into `clubs_playing`, `eleven_still_fit`, `settle_players` |
| | (4) `apply_match_report_in` 216 → 228 lines (pre-existing debt); `fold_old_seasons` has two near-identical halves; `side_events` 83 lines | guard extracted into `fixture_is_open`; the rest **not changed**, named for epic #589 |
| | (5) per-injury world scan in the dormant path | **not changed**: measured 2.5 µs a fixture on a two-club world; not profiled on an 8,000-player world |
| | (7) `StatsState::append` silently drops totals | **fixed** by documentation (it takes captures) |
| | design call: is the retention window a `domain` value rule or an `ofm_core` policy? | **left in `domain`** (it owns the totals types and sums), flagged; moving the trigger to `StateManager::append_stats_state` is a defensible alternative |
| | clean: engine isolation, the single shared stage, save compatibility, no sequence that loses or duplicates match lines | |
| `ofm-dedup-reviewer` | (1) the banned-player check existed twice in the harness | **fixed**: in `Invariants` only |
| | (2) `deflate`/`inflate` twice in `db` | **fixed**: `db::compression` |
| | (3) goal → scorer mapping twice | **fixed**: `post_match::scorers_by_side` |
| | (4) `rested_eleven` beside `catchup::likely_eleven` | named constants added; **kept separate** (the ranking differs on purpose: a tired first choice is rested) |
| | (5) player/team totals code copy-pasted; (6) a 25-field stats-record test builder re-created in four crates; (7) `db` error constants repeated in ten repositories | **not changed**; named for #589 |
| `ofm-test-reviewer` | five tests that passed without exercising what they claimed (cards/injuries untested; scorers from the wrong club and self-assist; competition kind never persisted non-default; totals columns swappable; the season test could pass on a faulty run) | **all fixed**, each with a killed mutant (§17) |
| | yellow accumulation across a rollover untested; `suspension.rs` doc said a ban does not carry into the next season but `suspended_matches` is not cleared | **fixed**: test, and the doc now says a ban earned runs on and only the yellow count resets; stale test name `only_a_dismissal_suspends` renamed |
| | the dormant tier is outside the harness invariants (captures exist only for the full path) | **not changed**: the dormant tier is asserted by unit tests and by the tier comparison in the study, not by a gate rule |
| | equivalence test: `credited <= scored` named as equality; second finish refused because the session was taken, not by the guard; bans/injuries not compared across modes | renamed; **not changed** otherwise (the guard is pinned by `a_fixture_is_finalised_once`; the application-layer stale-command path is not separately tested) |
| | `world_evolution` excluded the user's club invisibly (15 → 11 seniors unexplained) | the club must now keep a side of 11 and the exclusion is explained in the test; drift bound tightened 2.5 → 2.0 |
| | `f50_storage` is entirely `#[ignore]`d measurement and not a regression guard | true: its guards are `stats_repo`/`replay_repo`/`edition_archive_repo` tests and the migration test; the study is how the numbers in §12 were made |

## 19. Remaining defects and limitations

* The lightweight tier has no substitutions, own goals, penalties, possession or shot/pass counts, and
  produces no player match lines (so a dormant club's players have season statistics but no per-match
  history). Chosen for storage; the cost is that nothing like a "recent matches" list exists for them.
* Yellow accumulation is a flat every-5th rule; the leagues' escalation (10, 15) is not modelled and the count
  is not per competition.
* AI starters under 50% condition: 1–4% (user club: 0%); `ai_select_starting_xi` rotation is thresholded.
* `cash_journal`, `retired_players`, `retired_managers` and `messages` grow ≈ 3.8 MB a season and are a
  third to a half of a 30-season file. Outside F5.0; the next storage target.
* The save rewrites the whole game every time (1.5 s at 30 seasons).
* Legacy saves are not compacted on disk until the freed pages are reused.
* The benchmark process reaches 1.5 GB resident at 30 seasons; the game and statistics are ≈ 130 MB, the
  rest is the harness holding copies. The in-app resident set at 30 seasons was **not** measured.
* No GUI playthrough (no display in this session); desktop saves from real play were not opened.
* `APFM_ENGINE_READ_TOKEN` does not exist as a repository secret (§22).
* The dormant tier is asserted by unit tests and by the tier comparison in the study, not by a harness gate
  rule (captures exist only for the full path). A club playing twice in one dormant day is defended but
  tested only at the helper level; the calendar's rest rule makes it unreachable in generated calendars.
* The application-layer stale-command path (a finish arriving for an already finalised fixture) is refused
  because the session is gone, not because of the finalization guard; only the guard's unit test covers
  the guard.
* `apply_match_report_in` is still ~228 lines and `fold_old_seasons` has duplicated player/team halves
  (existing debt and a design smell, named for epic #589); four reviewer duplications (player/team totals
  code, a shared stats-record test builder, shared `db` error constants) are also named for #589.
* The retention window and fold trigger live in `domain::stats`; an `ofm_core` home is defensible.
* A passive harness club can run its squad down to the floor over five seasons (11 seniors here); that is
  the game's design (it warns, it does not sign for the player) and is now asserted at ≥ 11.

## 20. F4.4 technical debt (unchanged, not attempted)

Per the brief, F5.0 does no football calibration. Still open from F4.3/F4.4: M12 scoring (≈ 4–5 goals a match
in career inputs against ≈ 2.5 for the statistical model: a high line still adds ≈ +0.9 goals), movement
volume (5–8× football's), shot volume and distance, numerical superiority, pressing, discipline, board
dismissals of the user's manager in M12 careers, and multi-season M12 play. `docs/APFM_RUN_F4_4_STRUCTURAL_VALIDATION.md`
remains the reference; its NO-GO stands.

## 21. Exact commits and branches

OFM `augustinpatte/ap-football-manager`, branch `claude/run-f5-0-universal-simulation`, from F4.4 head `67c67a77`:

COMMITS_PLACEHOLDER

M12 `augustinpatte/apfm-match-lab`, branch `f5.0/universal-simulation`: **no M12 change was needed**; the
branch points at `d4e6f32`, the F4.4 head, and that is the revision OFM pins. The pin was verified with the
local engine override moved aside: `cargo check --locked -p ofm_core -p db` finished against
`apfm-match-lab?rev=d4e6f329d7e48279a9dce5f7f3c1b970f7ff2131` (`ofm_core/Cargo.toml`, `Cargo.lock`), and
`git ls-remote` shows `f4.4/structural-validation` at the same SHA. M12's uncommitted
`Cargo.toml` licence change (`license = "GPL-3.0-or-later"`) and the untracked `LICENSE`/`NOTICE` files in M12
and in the viewer were left exactly as found. The viewer was not touched.

## 22. Push status and CI secret

PUSH_PLACEHOLDER

`APFM_ENGINE_READ_TOKEN`: `gh secret list --repo augustinpatte/ap-football-manager` returns **no secrets**
(checked without reading any value). The workflows that check out the private engine
(`nightly-e2e.yml`, `dependency-advisories.yml`, `mutation-tests.yml`, and the CI jobs that build the
engine) read it as `secrets.APFM_ENGINE_READ_TOKEN`. **User action:** create a token that can read the
contents of `augustinpatte/apfm-match-lab` (a fine-grained personal access token, Contents: read, that one
repository) and add it as the Actions secret `APFM_ENGINE_READ_TOKEN` in the OFM repository's
Settings → Secrets and variables → Actions. Until then those jobs cannot fetch the engine and will fail;
nothing in F5.0 depends on it.

## 23. GO / NO-GO for F5.1

Against the completion criteria (§15 of the brief), each stated as tested or not:

| | Criterion | Verdict |
|---|---|---|
| A | every active match processed correctly | **Pass** for 2 seasons × 3,856 full-engine matches (exactly one capture each; 0 totals faults). One seed. |
| B | AI clubs experience genuine consequences | **Pass**: bans 1,086 → 1,987, injuries 0.21–0.25 per team-match, rates within the engine's for the dormant tier too |
| C | all modes update the same career systems | **Pass**: three modes through one finalization; the dormant tier through the one consequence stage |
| D | suspensions and injuries affect eligibility | **Pass**: 0 banned and 0 injured players with minutes in ~104k lines; gate rule added |
| E | fatigue and recovery coherent across consecutive fixtures | **Partial**: coherent, no spiral (F4.2 interactive season test unchanged), but 1–4% of AI starters kick off under 50% condition against 0% for the user's club |
| F | player and team statistics reconcile | **Partial**: player appearances, minutes, goals and yellows reconcile to the match lines; reds and assists, and team lines against standings, were not reconciled separately (standings consistency is an existing daily gate rule) |
| G | calendar skipping cannot lose or duplicate fixtures | **Pass** by the existing fast-forward ≡ day-by-day suite, re-run on the final tree |
| H | save/reload preserves the complete career | **Partial**: stats, totals, archive, replays and the migrated history are tested through second readers; the 30-season reload asserts equal counts, not a deep comparison of the whole career |
| I | storage growth measured and controlled | **Pass**: §12 |
| J | long careers remain performant | **Pass with a caveat**: 1.5 s save / 1.0 s load at 30 seasons; the benchmark process reaches 1.5 GB resident and the in-app figure was not measured |
| K | no critical regression | **Pass**: 4,498 Rust tests (default features), 635 with `--features mcp`, 2,913 frontend tests, clippy clean under both feature sets; three existing tests encoded the old "dormant matches cost nothing" rule or a zero-margin trajectory and were rewritten (§5, §18) |

**GO for F5.1** on career integrity and storage, with these carried forward: the AI-rotation residue (E),
reds/assists and team-line reconciliation (F), a whole-career deep reload comparison (H), measuring the app's
own resident set at 30 seasons (J), the `cash_journal`/`retired_players`/`retired_managers`/`messages`
growth of ≈ 3.8 MB a season (§12), the secret in §22, and a GUI playthrough of a migrated v65 save (not
possible in this session).

**NO-GO stands for calling the football final** (§20): F5.0 did not touch the engine's scoring, movement,
shot or discipline calibration, and the 4–5 goal M12 matches against the statistical tier's ≈ 2.5 remain the
largest open difference between the simulation modes.

## Checks not run

`npm run build` (`tsc`), Biome (`npm run lint`/`format:check`) and `npm run audit:i18n` were not run: no
frontend file changed. The Tauri desktop app was not launched. No M12 test suite was run: M12 is unchanged
at the pinned revision. `cargo deny`/`cargo machete`/`knip` (dead-code gates) were not run. The
`#[ignore]`d studies were run by hand and are the source of §12, §13 and §15; they are not part of any gate.
