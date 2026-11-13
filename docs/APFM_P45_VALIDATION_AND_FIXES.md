# APFM P4.5: strategic AI validation and defect closure

Branch `claude/apfm-playable-career`, from `aec1af70` (P4). Not merged into `apfm/main`, not pushed. M12, the
external 2D viewer and the frontend were not touched.

Evidence labels: **RUN** (executed this phase, command in §12), **READ** (read in source), **REVIEW** (a
reviewer agent's finding, checked here).

## 1. Summary

| | Result |
| --- | --- |
| Reviewers | All three run. **Architecture: no blocking.** Dedup: 2 blocking. Test: 4 blocking. Every blocking finding fixed and mutation-checked; non-blocking ones fixed or listed in §10. |
| Long career | **30 seasons on 6 gate seeds (7 runs) and on 1 shipped seed (440 clubs, 13,865 players); 10 seasons on gate seeds 1-3 and the shipped seed.** Invariants clean everywhere after one fix (below). |
| Defects fixed | (1) promoted/relegated clubs misread (tier on the season record); (2) a suspended injured player could be called up from the treatment room (new, found by the 30-season run); (3) reviewer findings (rotation floor, keeper rule written twice, identity entry point, hash helpers). |
| Investigated, cause named | Club over 110% of wage budget (loan return, squad-floor minimum-wage signing, or a budget cut: by design, no rule changed); opening ratings disagree with attributes (`begin_career` makes positions granular without refreshing `ovr`: **not changed**, owner decision). |
| New defect, specified, not fixed | Reloading a save restarts the job-offer clock (`days_since_last_job_offer` is not persisted): the reloaded career diverges on its first day. Needs a schema change. |
| Measurement integrity | The model's greedy mirror is deleted. "Does a signing start?" is now asked of the real lineup picker at the moment of the deal and checked against what he then played. |
| Verdict | **GO** for the next strategic AI phase, with the conditions in §13. |

## 2. Commits (oldest first, on top of `aec1af70`)

| Commit | Change |
| --- | --- |
| `f47b9474` | `fix(p4)`: act on the three reviewers; retire the invalid model-versus-picker measurement |
| `397cf72f` | `fix(club-strategy)`: read a club's last finish for the division it was earned in (`TeamSeasonRecord.tier`) |
| `8fc969f5` | `test(ratings)`: pin down why opening ratings disagree with attributes, specify the fix (ignored) |
| `00dcd80d` | `feat(harness)`: the long-career study (`career_study`, `p45_long_career`) |
| `d72463a2` | `test(save)`: ten-season reload twin; the job-offer-clock gap specified (ignored) |
| `f8ee4d2e` | `fix(match-day)`: never call up a suspended player from the treatment room |
| (this report) | `docs(p45)`: this document and the status banners on the three P4 documents |

The reviewer-fix commit holds code, tests and the measurement rebuild together; they share files
(`squad_intelligence/model/mod.rs`, `lineup_gap.rs`) and could not be split without partial-hunk staging.

## 3. Reviewer outcomes

Reviewers were run on `e5282b67..aec1af70`.

### Architecture (`ofm-architecture-reviewer`): no blocking

| # | Finding | Outcome |
| --- | --- | --- |
| 1 | Rotation disposition could drop the rest line below `EXHAUSTED` (a quality-0, rarely-rotating manager sat at 42) | **Fixed**: threshold clamped at `EXHAUSTED`; test at condition 43, mutation-checked |
| 2 | `ManagerIdentity` / `manager_identity` were `pub` with no external caller | **Fixed**: `pub(crate)` |
| 3 | `lineup_gaps` / `LineupGap` are public API for ignored studies, with no unit test | **Test added** (three cases); kept public for the harness, listed in §10 |
| 4 | Size limits: `ai_select_starting_xi_by` ~137 lines (was ~136), `build_team_with_bench_from` 122, `gap_of` 97, `profiles` 99 | Pre-existing debt, listed in §10 |
| 5 | "Model and picker solve the one assignment" overstated | **Fixed**: comments now say "to within rounding" and name what differs |
| 6 | `worth_borrowing`'s must-start test also applies to hard-sale listings of young prospects | **Documented** as deliberate in the docstring |
| 7 | Ignored test without an owner reference | **Superseded**: the promoted-champion test is un-ignored and real |

### Dedup (`ofm-dedup-reviewer`): 2 blocking

| # | Finding | Outcome |
| --- | --- | --- |
| B1 | Keeper bonus, resolution and cost rule written in `model` and again in the picker; the outfield-keeper cap only in the model | **Fixed**: `assignment.rs` owns `UNCOVERED_FIT`, `level_in_slot`, `slot_cost`; both call them. The picker now caps an outfield keeper as the model does (test fails without it) |
| B2 | "The user's club is read neutral" written inline in one place | **Fixed**: `ManagerIdentity::for_club` is the one place; the picker and `profiles` use it (the weekly review keeps `of_club_manager`: the user's club is not in its list) |
| N1 | Hash-to-unit draw copied; the finaliser belongs with `stable_hash` | **Fixed**: `unit_interval`, `mixed` in `stable_hash.rs`, with tests |
| N2 | `p4_results` re-derived the identity with nothing pinning it | **Fixed**: pinned values in both the game and the study |
| N3 | `env_list` pasted six times | **Fixed** for the p4 studies (`season_harness::env`); five older copies remain (§10) |
| N4 | Three slot-walk copies; the model's greedy mirror stale | **Fixed**: mirror deleted (§9) |
| N5, N6 | The user's team still selects greedily; a wrapper kept for 17 test call sites | Listed in §10 |

### Test (`ofm-test-reviewer`): 4 blocking

| # | Finding | Outcome |
| --- | --- | --- |
| 1 | The keeper test could not fail (11 players, no alternative) | **Fixed** (keeper rated 5, a spare outfielder); verified red with the bonus removed |
| 2 | The storage-order test had no ties | **Fixed** (a clone under another id); verified red with the id sort removed |
| 3 | Seven production changes had no test that failed without them (the sale guard, three identity call sites, blunt-side adaptability, the user-club branch, congested halving) | **Fixed**: wiring tests drive the real selection, weekly review, profile and sale paths with opposite managers. Each of 7 mutants was removed in turn and its test went red; the congested-halving mutant first survived, the fixture was sharpened, and it was killed |
| 4 | "Neutral" tests compared a function with its own wrapper | **Fixed**: pinned to the literal line (`50 + 25q` → 62 rests, 63 rides) and the measured triggers |

Non-blocking test findings fixed: the trap test's doc, the short-squad assertion (now checks the prefix), a
positive control on the sale test, the neutral youth assertion (now a literal), the loan-horizon boundaries
(365/366/730/731 days and an unreadable end date). Left: the pairwise-swap test is a weak local-optimality check
(its doc says so); some of the seven club-situation tests overlap older ones.

## 4. Defects

### 4.1 Promoted and relegated clubs misread (fixed, `397cf72f`)

A season record stored the finishing position but not the division. `profiles()` read last season's finish against the
club's *current* league, so a promoted champion was lifted to Established and a relegated bottom club marked down twice.

- `TeamSeasonRecord.tier: Option<u32>` (0 the top flight), `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  It lives in a JSON column, so **no SQL change, no migration, no backfill**. A record from an older build, or one with no
  nameable division, reads as it always did; a record with no tier writes no new key.
- `end_of_season` writes the tier it already computed for prize money; the ranking is extracted once (`ranked_divisions`,
  `division_tiers`) so payout and strategy read one answer.
- Tests at the real layer (`profiles`): promoted champion stays a fighter; relegated club stays a contender; same-division
  finish still lifts; legacy record reads as before (the last two pass before the change as controls). Plus: old JSON loads
  without a tier, a tier round-trips JSON and SQLite, an absent tier adds no key, the rollover records tier 0 and 1 for a
  two-division season. Writer and reader each removed in turn: red.

### 4.2 A suspended, injured player could be fielded (new, fixed, `f8ee4d2e`)

Found by the 30-season run: seed 2, season 27, day 178, `banned-player-never-plays` fired. The player was the harness's idle
user club's young left winger with a one-match ban and an eight-day calf strain; he played 93 minutes.

Cause (**READ**, reproduced by a probe): `eligible_among` adds every *injured* player to the match-day pool when fewer than
eleven can play, and did not exclude those also serving a ban. The game's own rule is that suspension has no emergency
exception. **Not caused by P4** (`eligible_among` is untouched since `e5282b67`). Fix: the emergency call-up excludes the
suspended. Test red for the predicted reason (three players both injured and banned were in the pool), green with the fix.
After the fix seed 2 is clean for 30 seasons (§6).

### 4.3 Club over 110% of wage budget: cause named, no rule changed

The one club at the end of season 1 in P4's 20-seed run was **Independiente Santa Fe, seed 6, day 203**: bill 106.2% of
budget the day before, 110.2% after. No wage figure changed: a player's contribution went from 2,549 to 10,196 a week (25% to
100%), i.e. **his loan from the club ended** and the parent's share returned. The club was already 106% over.

Across 10 seeds × 10 seasons (2,826,620 club-days, **RUN**) clubs exceeded 110% on **575 club-days (0.020%)**, in 9
episodes of mean 45–69 days and peak 111.4%:

| Cause | Episodes | Mechanism (all **READ** to be by design) |
| --- | --- | --- |
| Free-agent signing at the 500 minimum wage | 6 | The squad floor's emergency signing is exempt from the AI ceiling (`ai_finance_guard.rs`: "only the squad floor's cheapest signing goes past"). Clubs were already at 109.5–110.8% |
| Loan return | 2 | The parent's wage share returns in full (`finances::weekly_share`), 2–4% of budget |
| Wage budget cut at rollover | 1 | Budget falls, bill does not |

AI clubs' own signings stop at 100% (`AI_WAGE_CEILING_PCT`). The 110% is the board's soft cap for contracts a manager must
make. No financial rule was modified. Over 30 seasons on gate seeds 1-3 only 4 of 7,110 club-seasons ended a season over 110%.

### 4.4 Opening ratings disagree with attributes: cause named, **not changed**

Stored `ovr` against the rating the attributes give at the natural position (`natural_ovr`), **RUN** on a generated world:

| Stage | Players | Mean offset | Beyond rounding |
| --- | --- | --- | --- |
| Generated world data | 11,880 | +0.00 | **0.0%** |
| After `begin_career` (gate game) | 3,744 | −1.52 (|offset| 2.23) | **41.4%** |
| By day 100 | 1,933 | 0.25 | 0.0% |

The offset is entirely positional: keepers, centre backs, central midfielders and strikers are unaffected; left/right
wingers (−8 to −10), wing backs (−7 to −10), wide midfielders (−6), defensive and attacking midfielders (−5) and full backs
(−1.2 to −1.4) are not. **Cause:** step 6 of `begin_career` (`upgrade_game_player_identities`) makes each coarse position
(Defender/Midfielder/Forward) granular after `ovr` was computed for the coarse bucket, and does not refresh `ovr`. A later
refresh (training, ageing) corrects it, hence day 100.

**Not fixed, on purpose.** Refreshing there moves the rating of about two fifths of outfield players by up to ten points, and
with it their wages, values and the squad model's day-one reading: a rebalance of the opening world, the one thing the brief says
not to do silently. `ofm_core/tests/opening_ratings.rs` has two passing tests (the generator is consistent; only the granular
positions disagree after the career begins) and an **ignored specification** that every opening rating agrees with the
attributes. The rating formula is untouched. Owner decision in §13.

### 4.5 Reload restarts the job-offer clock (new, specified, not fixed)

`p45_reload_twin_after_a_long_career` saves a ten-season career through SQLite, reloads it, and plays both copies on. It failed
on play day 1: the original sent a job offer, the reloaded copy did not. **Cause:** `Game::days_since_last_job_offer` is never
written (the field is in `game.rs`; no persistence code names it), so a load restarts the clock. Specified by the ignored test
`test_save_and_load_roundtrips_days_since_last_job_offer` (fails today: `None` against `Some(6)`). The fix is a `game_meta`
column, a migration and its pinned hash, a schema change left for the owner.

Comparing every top-level field of the saved and reloaded `Game` after ten seasons (**RUN**), the only things that differ:

| Field | Difference | Reaches play? |
| --- | --- | --- |
| `days_since_last_job_offer` | 6 against none | **Yes** (first divergence) |
| `national_teams` | list order not kept; every team keeps the same fixtures by id | No (60 days) |
| `players` | 21 of 3,450 have `weak_foot` re-inferred at load (2 → 3) | No (60 days) |
| `staff` | 169 of 489 gain `birth_country`/`football_nation` at load | No |
| `world_history` | a float differs in its last digit | No |
| `squad_floor_top_ups` | session diagnostic, documented as never saved | n/a (harness comparison now excludes it) |

With the timer carried across by hand, **three seeds are identical after the reload and identical sixty days on**.

## 5. What the long career shows

Worlds: the 80-club gate world and the shipped 440-club world, deterministic seeds, release profile, the invariants
observer on throughout. `p45_long_career` prints one row a season; figures below are summed or averaged over seeds as stated.

### 5.1 Gate world, 30 seasons: seeds 1, 2, 3 (before the §4.2 fix) and 2, 4, 5, 6 (after it)

| Check | Result |
| --- | --- |
| Completed | 7 runs of 30 seasons (6 distinct seeds), about 134-150 s each, no slowdown (4.5 s a season, flat) |
| Invariants | Clean in 6 of 7; seed 2's pre-fix run hit the suspension breach (§4.2); **seed 2 re-run after the fix: clean** |
| Squad sizes | Mean 22.0 (season 1) → 23.4 (30); min 18-20, max 29-32 every season; **no thin squad, no club without a keeper, no club with one keeper, ever** |
| Finances | **No club in debt in any season**; median wage/budget 1.00 from season 1 |
| Competition continuity | 5 leagues, size never changes, **no unfinished league**, 4 promoted and 4 relegated every season from season 2 |
| Managers | ~42 permanent managers leave and ~42 are hired a season from season 5 (about 0.5 a club a season, mean tenure about two seasons); mean vacancy 42-50 days; up to 15 clubs under an interim at a season's end |
| Formation diversity | **One formation, 100% of clubs, no change in any season** (§10) |
| Tactical plans | 34 distinct (season 1) → 26 (season 30) |

Aggregated per season (seeds 1-3):

| Season | Prospects (n) | Growth | Minutes | Under 450 min | Idle loans, for-minutes | Idle loans, others |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 770 | +3.16 | 533 | 77% | 9/25 | 26/203 |
| 3 | 685 | +5.04 | 860 | 72% | 14/62 | 23/102 |
| 10 | 414 | +5.17 | 415 | 84% | 16/38 | 57/122 |
| 20 | 390 | +4.93 | 371 | 86% | 12/23 | 89/168 |
| 30 | 387 | +5.06 | 306 | 87% | 19/37 | 99/170 |

### 5.2 Shipped world, 30 seasons, seed 1 (440 clubs, 13,865 players; pre-fix binary)

**Completed in 1,611.7 s (26.9 minutes), invariants clean for all 30 seasons.** Per season 36 s, then flat at about 54 s.

| Check | Result |
| --- | --- |
| Squad sizes | Mean 22.0 → 23.5, min 18-20, max 25-32; no thin squad, no keeper shortage, no club in debt in any season |
| Wage budget | Over 110% at a season's end for 1 club in seasons 2, 25 and 30 |
| Pyramid | 23 leagues; 24 promoted and 24 relegated every season from season 2; no unfinished league |
| Managers | 18-49 permanent managers leave a season; vacancy 42-52 days |
| Formations | One, 100%, no change (as the gate world) |
| Tactical plans | 49 → 33 |
| Prospects | 4,328 (season 1) → 2,202 (season 30); minutes 437 → 297; under 450 minutes 82% → 88% |
| Idle loans (2+ months, under 180 minutes) | for-minutes players 14/31 → 48/62; others 36/376 → 242/328 (**about 75% idle by season 30**) |

The shipped run used the binary from before the §4.2 fix and was clean. A separate 10-season shipped run is also clean
(511 s, 507 MB).

### 5.3 What carries over from P4, and what does not

The P4 loan rule **holds over ten seasons** (6 gate seeds, `p4_loans`, **RUN**; the baseline is `e5282b67`):

| | Baseline | P4.5 |
| --- | --- | --- |
| Loans ended | 5,106 | 3,524 |
| Idle loans, all | 50.6% | **29.8%** |
| Age 21 or under | 58.1% | **32.3%** |
| Room to grow (6+) | 62.8% | **32.2%** |
| Minutes per loan day (all / ≤21) | 4.4 / 3.6 | **7.2 / 7.3** |

But the long career shows what the three-season P4 study could not: **idleness rises over a career in every band** (about a
quarter in season 2, about half by season 10, and about three quarters in the shipped world by season 30), the **prospect cohort halves**
(gate 770 → 387, shipped 4,328 → 2,202), and prospect minutes fall (gate 860 → 306). P4 made the loan decision
better; it did not stop the youth pipeline from thinning, and nothing in P4 was meant to.

The market is also nearly silent in seasons 1-2 (0.04 deals a club) and busy from season 3 on (0.6 in season 3, 1.8 in season
5), so **two-season studies measured the quietest part of the market**. The P4 20-seed result (unused joiners −1.1 points) was
measured there; at ten seasons the market metrics of the two trees are indistinguishable at three seeds (below).

### 5.4 Ten-season market comparison against `e5282b67` (gate, 3 seeds, the same `f54_squad_study`)

| Season | XI fit | Weak-slot clubs | Seniors | Joiners / club | Unused joiners | Fees / club (M) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 68.10 → 68.10 | 19.0% → 18.7% | 22.1 → 22.0 | 1.30 → 1.30 | 9.3% → 7.3% | 0.05 → 0.05 |
| 3 | 68.47 → 68.47 | 17.3% → 16.7% | 24.7 → 24.7 | 3.57 → 3.67 | 13.0% → 16.3% | 0.96 → 0.97 |
| 5 | 69.27 → 69.20 | 9.7% → 8.7% | 24.4 → 24.2 | 3.83 → 3.83 | 14.7% → 15.3% | 1.93 → 1.72 |
| 10 | 70.50 → 70.50 | 1.7% → 1.7% | 23.3 → 23.2 | 2.57 → 2.63 | 21.0% → 20.3% | 1.59 → 1.70 |

No difference in squad quality, positional fit, squad size or spending at ten seasons; the unused-joiner movement either way is
within three-seed noise. **P4 changed consistency and loans, not the market's output, and the report should not claim more.**

### 5.5 Start on arrival: the model's prediction against the real picker (the rebuilt measurement)

For deals in season 3 of seed 1 (49 signings): the model said **21** would start on arrival, the real picker started **23**,
**both agreed on 21**, the model alone claimed 0, the picker alone 2. Of those the model said would start, 16 of 21 played three
or more matches; of those it said would not, 10 of 28 did (depth and succession signings are expected to play, and some do).
This is the check P4 lacked, and it supports P4's central claim: the market now prices a signing for the start the picker gives
him.

## 6. Save and reload

| Check | Result |
| --- | --- |
| Existing four-season SQLite reload at every stage (`season_lifecycle`) | Passes |
| Ten-season career saved to SQLite, reloaded, played on 60 days (3 gate seeds) | Identical before and after, once the job-offer timer is carried across (§4.5) |
| Differences a reload makes | The timer (reaches play), plus four that do not (§4.5 table) |
| New persisted state in P4/P4.5 | One `serde(default)` field in an existing JSON column (`TeamSeasonRecord.tier`); no schema change |

## 7. Runtime and memory

Release profile as committed, `/usr/bin/time -l`, shared machine (another worktree compiled throughout, so wall time is
indicative; **instructions retired** is the primary figure, as in the F5.5 report).

| Run | Instructions | Wall | Peak RSS |
| --- | --- | --- | --- |
| `e5282b67`, gate, 3 seeds × 10 seasons | 2,389.9B | 123.0 s | 190.9 MB |
| P4.5, same | 2,400.2B (**+0.4%**) | 124.5 s (+1.2%) | 197.2 MB (+3.3%) |
| P4.5, gate, 3 seeds × 30 seasons, invariants on | 7,685B | 403 s (134 s a seed) | 656 MB (three seeds, logs kept) |
| P4.5, shipped, 1 seed × 10 seasons, invariants on | 7,872B | 511 s | 507 MB |
| P4.5, shipped, 1 seed × 30 seasons, invariants on | 23,372B | 1,612 s | 1,177 MB |

The F5.5 optimisations are preserved (−26.9% instructions against F5.4; P4.5 is within half a percent of the F5.5 tree at ten
seasons). Memory in the 30-season runs is the studies' own logs, which keep every deal and season row.

## 8. Tests

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean |
| `cargo clippy --locked --workspace --all-targets --features mcp -- -D warnings` | clean |
| `cargo test --locked --workspace --no-fail-fast` | **130 targets, 4,784 passed, 0 failed, 86 ignored** (P4: 128 / 4,756 / 83; the baseline `e5282b67`: 122 / 4,725 / 76) |
| `cargo test --locked --lib --features mcp` | 653 passed, 0 failed, 8 ignored |
| `npm test` | 340 files passed, 1 skipped; **2,993 tests passed**, 4 skipped (unchanged) |
| `npm run build`, `npm run lint`, `npm run format:check`, `npm run knip` | all exit 0 |

The ignored count rose by three against P4: two ignored specifications (the opening-rating gap, the job-offer clock), two new
`#[ignore]`d long-career studies, less the promoted-champion specification, which is now a real, passing test. The frontend
suite was run although no frontend file changed, as asked. Not run: `cargo deny`, `cargo machete`, and the `/preflight` skill
as one invocation (its component commands above were each run).

**Tests added this phase** (all red-first or mutation-checked): picker (keeper rule, tie order, short squad, keeper cap) and
rotation (floor at 43, neutral line, congested halving, wiring through the real selection path); identity (pinned values,
user-club neutrality, weekly-review wiring, blunt-side adaptability, neutral at the measured lines); profiles (manager wiring,
four tier cases); `TeamSeasonRecord` (old JSON, round trip, absent key); SQLite round trip with a tier; the rollover
recording tiers; the loan horizon boundaries; `lineup_gap` (three cases); the sale guard in `ai_contracts`; `season_harness::env`;
`stable_hash` helpers; the suspended-and-injured call-up; the opening-rating characterisation.

## 9. Measurement integrity

- Deleted: `greedy_picks_with`, `Placement.greedy_start`, `LineupGap.mirror_raw` and `mirror_disagreements`. The mirror never
  matched the picker (3.8-3.9 slots a club apart at the opening, about one later) and no longer mirrored anything.
- Rebuilt: `DealPlacement.picker_starts_now` asks `first_choice_eleven` itself, over the buyer's healthy seniors and the signing,
  when the deal is recorded. The study prints "start on arrival: model says / picker starts / both / model only / picker only"
  and checks each against matches played. The succession line no longer prints a picker column (a future squad cannot be asked):
  it says so and defers to appearances.
- Marked invalid: every "greedy starts" and "model-only pathway" figure in the P4 documents is retired; each of the three P4
  documents now carries a banner saying so.
- The model's anchored, integer-rounded fits still differ from the picker's raw continuous ones: 0.15-0.33 starters a club by
  `lineup_gap`, zero in raw rating. Stated in the picker's doc comment; not "the same function".

## 10. Remaining technical debt (non-blocking)

1. **Every AI club plays one formation for ever** (gate and shipped, 30 seasons, no change). The largest gap in "clubs set themselves up".
2. **The youth pipeline thins over a career** (§5.3) and loans stay about half to three quarters idle in the long run.
3. The user's own club still selects greedily (`select_starting_xi` pass two, `reseat_by_position`): two "best XI" rules by who manages.
4. `lineup_gaps` / `LineupGap` are public for the studies and ship in the app build; gate behind a harness feature when a second consumer appears.
5. Pre-existing size debt, touched by a few lines and not decomposed: `ai_select_starting_xi_by` (~137), `build_team_with_bench_from` (122), `evaluate_transfer_market` (~900).
6. `season_harness::env` is used by the p4/p45 studies; `f50`, `f51`, `f53`, `f54` still carry their own copies of the same parsing.
7. `ai_select_starting_xi` remains as a wrapper for 17 test call sites.
8. Two ignored specifications: the opening-rating gap and the job-offer clock (§4.4, §4.5).
9. The `f54_squad_study` "model-only pathway" and "greedy starts" columns are gone, but its older reports in `docs/` still print them; they are historical.

## 11. Remaining blockers

None for backend work. Carried over and unchanged: **no person has played the window** (P2.5 and P3 carry the manual GUI
checklist), and the two owner decisions in §13. The frontend was not changed.

## 12. Reproduce

```sh
# from src-tauri/, release profile
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=30 cargo test --release -p season-harness --test p45_long_career -- --ignored --nocapture
P45_WORLD=shipped P45_SEEDS=1     P45_SEASONS=30 cargo test --release -p season-harness --test p45_long_career -- --ignored --nocapture   # ~27 min
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_reload_twin -- --ignored --nocapture
P4_SEEDS=1,2,3,4,5,6 P4_SEASONS=10 cargo test --release -p season-harness --test p4_loans -- --ignored --nocapture
F54_WORLD=gate F54_SEEDS=1,2,3 F54_SEASONS=10 cargo test --release -p season-harness --test f54_squad_study -- --ignored --nocapture
# run a built test binary under /usr/bin/time -l for instructions and peak memory
```

`P45_INVARIANTS=0` skips the rule-checking observer. A 30-season gate run is practical (about 2.5 minutes a seed); a
30-season shipped run took 26.9 minutes. **The 30-season gate is therefore verified, not "unverified".**

## 13. Decision

### GO for the next strategic AI development phase

Why: the AI and the world hold up over a career. Thirty seasons on the gate world (6 seeds) and on the shipped world (1 seed)
finish with clean invariants (after one pre-existing defect the run found and this phase fixed), flat per-season cost, no
financial or squad-size failure, stable pyramids, and a reload that plays on identically once its one known gap is bridged.
P4's central consistency claim is now checked against the real picker.

Conditions, in order:

1. **Owner decisions** (neither blocks AI work, both affect the opening world and saves): whether `begin_career` should refresh
   `ovr` (§4.4); whether to persist the job-offer clock with a migration (§4.5).
2. **Run the 10-season gate comparison again on any change that touches lineups or loans**; the two-season studies measure the
   quietest part of the market (§5.3).
3. The next phase's natural targets are the ones the long career names: formation choice (§10.1), a youth pathway that plays
   prospects and keeps the pipeline full (§10.2), and sales and renewals that read the club's strategy. Do not begin them without
   a ten-season baseline, which this report provides (§5).
4. The standing GUI condition from P2.5 and P3 is unchanged.

The next phase was not begun in this session.
