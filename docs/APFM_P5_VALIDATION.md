# APFM P5: validation

Companion to `docs/APFM_P5_MATCH_REALISM_AND_FINAL_AI.md` (what was found and changed). This document is the evidence and its limits.
Branch `claude/apfm-playable-career`, baseline `4d7e7acf`, final tree `74d95613` (the long runs are on `e2c74619`; section 9).

Evidence labels: **RUN** (executed this phase, command in section 10), **READ** (read in source).

**Independent units.** The unit of independence is the **world** (the seed) for anything about a career, and the **club** for anything about
matches (the matches of one club share its squad). The match studies use **3 worlds × 8 clubs = 24 subject clubs** (12 worlds × 8 for the
absence study); the career studies use **3 gate worlds (seeds 1-3) and 1 shipped world**. Matches (18,432 in the main study) are
never the unit: a table's standard error is the spread of per-club contrasts. The career studies are **same seeds, different
trajectories** against P4.7: a different decision, or a different engine, changes every later draw, so a difference is not a controlled
one. **The engine changed in this phase (the goal rate), so P4.7's published figures are the comparison, not a paired baseline;
no baseline worktree was rebuilt.**

## 1. What was run

| Run | World | Seeds | Seasons | Result |
| --- | --- | --- | --- | --- |
| Workspace tests, baseline `4d7e7acf` | | | | 140 targets, 4,856 passed, 0 failed, 94 ignored (= P4.7) |
| Controlled formation and dial study `p5_formation_matches` | gate, warmed 60 days | 1, 2, 3 | | 24 clubs, 18,432 matches (section 4) |
| Absence study `p5_absence_formation` | gate, warmed 60 days | 1-12 | | 96 club-scenarios a case (section 4.3) |
| Reference seasons `p5_reference_season` | gate | 1, 2, 3 | 1 | 2.48 / 2.56 / 2.52 goals a match, 26 % drawn (whole world: followed + dormant) |
| Ten-season study `p45_long_career`, invariants on | gate (80 clubs, ~3,740 players) | 1, 2, 3 | 10 | invariants **clean** ×3 |
| Thirty-season study | gate | 1, 2, 3 | 30 | invariants **clean** ×3 |
| Shipped world | 440 clubs | 1 | 30 | section 6 |
| Reload twin `p45_reload_twin` | gate | 1, 2, 3 | 10, then 60 days | **identical** ×3 |
| Targeted: `p47_formations`, `p47_youth_causes`, `p47_loans`, `p47_appointments` | gate | 1, 2, 3 | 10 | sections 3, 5 |
| Performance, invariants off, `/usr/bin/time -l` | gate | 1-3 and 1 | 10 and 30 | section 7 |

## 2. Match engine: the goal rate and its fix

| Measure (**RUN**) | Before | After |
| --- | --- | --- |
| Goals a match, played through `play_unwatched_fixture`, England First / Portugal, worlds 1-3 | 3.91-4.11 (every league 3.9) | 2.68, 2.68 / 3.09, 2.77 / 3.41, 2.91 |
| Shots / on target a match | 25-27 / 10.8 | 24-26 / 6.3-8.0 |
| 80-rated v 60-rated worlds (engine test, 400 matches each), goals a match | 4.38 v 3.46, ratio **1.26** | ratio **1.03** (the test band is 0.9-1.1; red at 1.32 with the old pivot) |
| Study matches, all 18,432: goals a match / 0-0 / five or more / subject win-draw-loss | 3.9 / ~1 % / 52 % / 43-44 % wins, 15-19 % draws | **3.18 / 4.5 % / 21.9 % / 42.6 / 22.4 / 35.0 %** |

Causes ruled out on the way, each measured (section 5 of the report): condition, warm-up and AI tactics, the club's reputation (and so the
manager's experience), the strength gap between the clubs, the leagues' attackers, and the path (the day's sweep against a direct call:
identical on 152 pairings, three ways).

The remaining gap: the dormant tier scores 2.4-2.5; the engine now 2.7-3.4. England First, the user's league, is the highest in each
world (seed 3: 3.41 against 2.91 for Portugal); the reason was not found and nothing was tuned to it.

## 3. Formations and tactics (**RUN**, `p5_formation_matches`, 24 clubs, 3 worlds)

Table in the report, section 6.2. The squad model's formation score against results: **−0.0214 ± 0.0209** points a game per rating point (se over 24
clubs); the model's best shape is the best performer for **12 %** of clubs (chance 12.5 %). A club's ppg varies by **0.185 ± 0.015** between its eight shapes
(one standard deviation); match noise alone would give about 0.13.

The shape AI clubs choose most (4-3-3: 56 % of clubs at season 1, 41-46 % from season 4 on, 44 % at season 10, in the P4.7 formation study at the final tree) is, in this engine, the
one that scored **worst** (−0.082 ± 0.037, about 2.2 standard errors): choice by the squad model has no feedback from results.

The P4.7 formation study at the final tree (3 gate worlds × 10 seasons, 2,370 club-seasons): 5-7 shapes in use every season, largest share
56 % → 44 %, **0.262 changes per club-season** (P4.7: 0.27), flip-flops 49 (7.9 % of changes; P4.7: 9.3 %), 63 % of changes within a month
of a new manager (P4.7: 63 %). Unchanged.

### 3.1 Absence study (`p5_absence_formation`, 12 worlds, 96 club-scenarios a case)

| Two best out | Share of clubs the rule moves | Rule − club's own shape, points a game | Where it moved |
| --- | --- | --- | --- |
| Defenders | 46 % | −0.004 ± 0.015 | −0.008 ± 0.033 (44 cases) |
| Midfielders | 43 % | −0.013 ± 0.015 | −0.031 ± 0.035 (41 cases) |
| Forwards | 60 % | **+0.043 ± 0.018** | +0.071 ± 0.030 (58 cases) |

"Best of the eight, on the same matches, minus the club's own" was +0.26 to +0.29: an upper bound, **inflated by choosing the maximum of eight noisy
values**, and not a gain any rule could collect. The rule's gain is zero in two of three scenarios and marginal in one; not enabled.

## 4. Career structure (**RUN**, gate worlds, same harness and seeds as P4.7)

### 4.1 Ten seasons, three seeds (2,370 club-seasons)

| | P4.7 final | P5 |
| --- | --- | --- |
| Invariants | clean | **clean** ×3 |
| XI fit, season 1 / 3 / 5 / 10 | 69.13 / 69.53 / 70.10 / 71.50 | 69.13 / 69.53 / 70.17 / 71.33 |
| Clubs with a weak starting slot, % | 8.7 / 3.7 / 4.3 / 0.3 | 9.0 / 5.0 / 4.3 / 0.7 |
| Seniors | 22.4 / 24.9 / 24.5 / 23.4 | 22.5 / 24.7 / 24.4 / 23.5 |
| Usable bench | 8.8 / 11.8 / 12.1 / 11.8 | 8.9 / 11.7 / 12.2 / 11.9 |
| Players joining a club a season | 1.30 / 3.33 / 3.40 / 2.07 | 1.30 / 3.37 / 3.43 / 2.20 |
| Unused joiners, % | 6.7 / 15.3 / 15.7 / 16.3 | 5.3 / 14.0 / 14.3 / 14.7 |
| Transfer fees a club (M) | 0.04 / 0.81 / 1.29 / 1.09 | 0.03 / 0.84 / 1.46 / 1.26 |

Squad quality is **unchanged** (every row is within the noise of three seeds). This phase did not change the squad model, so the table is a
check that the engine change and the departure fixes broke nothing, not an improvement.

### 4.2 Thirty seasons, three seeds (7,110 club-seasons)

| Check | P4.7 final | P5 |
| --- | --- | --- |
| Invariants | clean | **clean** ×3 |
| Clubs in debt, any season | 0 | **0 of 7,110** |
| Club-seasons over 110 % of the wage budget | 1 (baseline 8) | **6** (0.08 %); not investigated; the P4.5 causes (a loan return, a squad-floor minimum-wage signing, a budget cut) are the candidates |
| Managers leaving / hired | 3,441 | 2,851 / 2,817 (a different trajectory; no claim either way) |
| Distinct formations, season 1 / 10 / 20 / 30 | 6.0 / 6.0 / 6.3 / 6.7 | 5.7 / 6.0 / 6.7 / 5.7 |
| Largest share, season 1 / 10 / 20 / 30 | 57 / 43 / 39 / 38 % | 56 / 41 / 43 / 42 % |
| Squads (followed clubs, every season of three runs) | min 18-21, max 29-32, no thin squad, no club without a keeper | min **19**, max **32**, **no thin squad, no club without a keeper, no club with one** (mean 22.4 → 23.5) |

### 4.3 Reload and determinism

**Reload twin, 10 seasons, three seeds:** save through SQLite, reload through a fresh manager, play both copies 60 days on: **identical before
and after on all three seeds**; job-offer clock `Some(0)`, `Some(3)`, `Some(1)` carried by the save; invariants clean. The cosmetic national-team
list order (not kept: false, as in P4.7) reaches no play. The two controlled studies reproduced digit for digit between the first and the final run
(same commit logic, 18,432 matches), and the three 30-season runs differ only by seed.

## 5. Youth, loans and appointments (**RUN**, three gate worlds × 10 seasons)

### 5.1 Youth (`p47_youth_causes`)

| | Season 1 | 5 | 10 |
| --- | --- | --- | --- |
| Prospect cohort at the end | 1,968 | 1,711 | **1,247** (P4.7: 1,195; ±10 % is noise) |
| New to the world / matured out of the cohort / aged out (a season) | 0 / 48 / 172 | 544 / 500 / 153 | 427 / 387 / 32 |
| First-eleven-ready, **senior role**: n (mean minutes, share under 450) | 318 (1,979, 29 %) | 394 (2,623, 21 %) | 190 (2,076, 29 %) |
| First-eleven-ready, **academy role**: n (minutes) | 6 (0) | 13 (0) | 28 (0) |
| Not ready, senior / academy: share under 450 | 90 % / 100 % | 86 % / 100 % | 88 % / 100 % |

The cohort is a steady state fed by 1-3 players a club and drained by maturing; the prospects who play are the ones the squad model wants; the
academy ones the model does not want play 0 minutes by the match-day pool's design. Nothing was changed.

### 5.2 Loans (`p47_loans`)

| Loan start season | 1-3 | 4-6 | 7-9 | 10 |
| --- | --- | --- | --- | --- |
| Loans, P4.7 → P5 | 223 → 219 | 456 → 494 | 404 → 328 | 178 → 162 |
| Idle at 60+ days, P4.7 → P5 | 12 → 8 % | 16 → 20 % | 22 → 22 % | 26 → 22 % |
| Minutes per loan day, P4.7 → P5 | 10.13 → 10.40 | 9.81 → 9.75 | 8.57 → 8.80 | 8.04 → 8.30 |

Within noise; **no change is claimed.** The cause of an idle loan is the same as in P4.7 (arriving behind the borrower's starters: 90 of 99, 61 of 71, 28 of 35).

### 5.3 Appointments (`p47_appointments`)

847 appointments: misfit of the manager appointed **0.139** against the pool's 0.324 (P4.7: 0.142 / 0.325); 872 completed tenures, mean 456 days, median 247
(P4.7: 380 / 167). A different trajectory; **no tenure claim**, as in P4.7. No result measure is attached to an appointment.

## 6. Shipped world, 30 seasons (440 clubs)

One seed (1), 30 seasons, invariants on, 13,170 club-seasons, **2,199.7 s** (42 s a season at first, 80-135 s late, one 143 s season; wall time on a busy machine is not a
result). No paired baseline (as P4.7: it was not re-run on this world); the comparison is P4.7's published figures.

| Check | P4.7 final | P5 |
| --- | --- | --- |
| Invariants | clean | **clean** |
| Clubs in debt, any season | 0 | **0 of 13,170** |
| Club-seasons over 110 % of the wage budget | 9 (0.07 %) | **10 (0.08 %)** |
| Squads | min 19-20, max 29-32, none thin | min **18**, max **33**, no thin squad, no club without a keeper, none with one |
| Pyramid | 23 leagues, 24 promoted and 24 relegated every season from season 2, none unfinished | **23 leagues, 24 up and 24 down every season, none unfinished** |
| Distinct formations, season 1 / 10 / 20 / 30 | 7 / 7-8 / 7-8 / 7-8 | 8 / 7 / 7 / 7 |
| Largest share, season 1 / 10 / 20 / 30 | 64 % → 42 % | 64 / 45 / 41 / 38 % |
| Changes of shape, season 1 / 10 / 20 / 30 | 462 / - / - / 25 | 451 / 36 / 29 / 25 |
| Managers leaving / hired (30 seasons) | not stated | 952 / 942 |
| Loans ended (idle), season 10 / 30 | not stated | 192 (101) / 182 (109) |
| Prospects at the end, season 10 / 30 (growth, minutes, share under 450) | not stated | 2,394 (+5.38, 406, 84 %) / 2,271 (+5.21, 294, 89 %) |

Prospect minutes and the share under 450 are **not improved** and are not claimed to be (the report's youth analysis says why).

## 7. Runtime and memory (**RUN**, instructions retired, invariants off)

| Run | P4.7 final | P5 | Change |
| --- | --- | --- | --- |
| Gate, 3 seeds × 10 seasons | 2,547.5 B, 190.3 MB | **2,538.4 B, 194.1 MB** (peak footprint 163.2 MB) | −0.4 % instructions, +2 % resident |
| Gate, 1 seed × 30 seasons | 2,553.4 B, 661 MB | **2,538.6 B, 672.6 MB** (peak footprint 620.2 MB) | −0.6 %, +2 % resident |

Flat against P4.7 (+4.6 % over the pre-P4.7 tree, so the F5.5 saving is preserved: about 24 % below F5.4). The refill and the reservations add nothing
that is not read only when a starter leaves or a bid is made; the engine change replaces one constant with a mean over the defenders on a shot
(about 100 shots a match). Wall time is not a result (the machine ran other work: 144-147 s for both gate runs).

## 8. Technical gates (**RUN**)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, and with `--features mcp` | clean |
| `cargo test --locked --workspace --no-fail-fast` (tree `74d95613`) | **142 targets, 4,905 passed, 0 failed, 99 ignored** (P4.7: 140 / 4,856 / 94; the new tests, and five ignored studies in two new targets) |
| `cargo test --locked --lib --features mcp` | **653 passed**, 0 failed, 8 ignored (unchanged) |
| `npm run lint`, `npm run format:check`, `npm run build` | all exit 0 |
| `npm test` | 340 files passed, 1 skipped; **2,993 tests passed**, 4 skipped (unchanged: no frontend file was touched) |

**Mutations run** (remove only the fix, see red, restore, see green; one mutation script left `bids.rs` half-mutated once, which I restored by hand and
re-verified by the full suite): the refill call in release/expiry, aging,
sale, loan return, loan out and a retired loanee; the fee reservation, the wage reservation, and "reserve every offer, not only pending ones"; the
risk shift; the injury-replacement call; the 50-pivot (ratio 1.32; and in the one-shot engine); the adapter's formation; the persistence round trip; the
match-day rule's guard and margin; the contender's reinvestment bar; and, after the test reviewer, the loan-out and retired-loanee refills, the fit-against-rating choice
(by id instead), the `can_play` filter (it **survived** until its fixture was thin enough to reach it; then red), the personality cutoff (0.25 to 0.30) and the play-style
chase threshold. The test reviewer repeated the main ones independently in a disposable worktree.

**Not run:** `cargo deny`, `cargo machete`, `npm run knip`, `npm run audit:i18n`, and the `/preflight` skill as one invocation; the M12 repository's own tests
(M12 was not modified). No player-visible string was added (every new refusal reuses an existing translation key), so the i18n and accessibility reviewers
were not required; no frontend file changed.

## 9. Commits validated

The long runs, the clippy run and the architecture and dedup reviews are on `e2c74619`. The commit after it (`74d95613`) changes **tests only**: every
production-source difference from `e2c74619` sits after the first `#[cfg(test)]` of its file (`shared.rs`, `live_match_manager.rs`, `spatial_runner.rs`, `turn/squad.rs`).
The workspace and MCP runs in section 8 and the clippy and format runs are on the final tree.

## 10. Reproduce

```sh
# from src-tauri/, release profile
P5_SEEDS=1,2,3 P5_CLUBS=8 P5_PANEL=6 P5_REPS=8 P5_WARMUP=60 cargo test --release -p season-harness --test p5_formation_matches p5_formation_matches -- --ignored --nocapture
P5_SEEDS=1,2,3,4,5,6,7,8,9,10,11,12 P5_CLUBS=8 P5_REPS=8 P5_WARMUP=60 cargo test --release -p season-harness --test p5_formation_matches p5_absence_formation -- --ignored --nocapture
P5_SEEDS=1,2,3 cargo test --release -p season-harness --test p5_formation_matches p5_reference_season -- --ignored --nocapture
P5_SEEDS=1 P5_LEAGUE="England First" P5_PROBE_OPP=19 cargo test --release -p season-harness --test p5_formation_matches p5_probe_match_paths -- --ignored --nocapture
cargo test --release -p season-harness --test p5_formation_persistence      # not ignored; also runs in the workspace
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=30 cargo test --release -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture
P45_WORLD=shipped P45_SEEDS=1     P45_SEASONS=30 cargo test --release -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_reload_twin -- --ignored --nocapture
for t in p47_formations p47_youth_causes p47_loans p47_appointments; do P47_SEEDS=1,2,3 P47_SEASONS=10 cargo test --release -p season-harness --test $t -- --ignored --nocapture; done
# instructions and memory: copy the built binary out under a new name, then
/usr/bin/time -l env P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 P45_INVARIANTS=0 ./p45_long_career-copy p45_long_career --ignored --nocapture
```

## 11. Limits of this evidence

1. **Three career worlds** (one shipped) and **24 subject clubs** for the formation experiment; 12 worlds for the absence study. The tables say so.
2. **The engine changed**, so a comparison with P4.7 is across engines, same seeds, different trajectories. No paired baseline was rebuilt.
3. **The five-zone engine only.** M12's football, its formation geometry and its scoring were not measured; the unwatched path and the long-career
   studies do not use it. The dormant tier (outside the followed scope) is not the engine at all.
4. **The study's panel is mid-table** and the subjects span the league; the formation effects are those of this panel, warmed 60 days into a season.
5. **The match-day shape study selects on noise** in its upper-bound column; only the rule's own column is a measurement of a decision.
6. **6 club-seasons over 110 % of the wage budget** in 7,110 were not investigated.
7. **The strategy listing wire** (report, section 9, C) has no behavioural test.
8. **No person has played the window**, and this session could not.
