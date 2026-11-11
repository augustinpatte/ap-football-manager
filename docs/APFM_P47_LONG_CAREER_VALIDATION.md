# APFM P4.7: long-career validation

Companion to `docs/APFM_P47_STRATEGIC_AI_COMPLETION.md` (what was built and why). This document is the evidence and its limits.
Branch `claude/apfm-playable-career`; baseline `b5430826` (P4.6), final `6667ef60`. Not pushed.

Evidence labels: **RUN** (executed this phase, command in section 11), **READ** (read in source).

## 1. What was run

| Run | World | Seeds | Seasons | Observer | Baseline paired? |
| --- | --- | --- | --- | --- | --- |
| Ten-season study (`p45_long_career`) | gate, 80 clubs, ~3,740 players | 1, 2, 3 | 10 | invariants on | **yes**, `b5430826` in its own worktree and target directory |
| Thirty-season study (`p45_long_career`) | gate | 1, 2, 3 | 30 | invariants on | **yes**, rebuilt baseline, same seeds |
| Shipped world (`p45_long_career`) | 440 clubs, 13,865 players | 1 | **30** | invariants on | **no** (section 6) |
| Reload twin (`p45_reload_twin`) | gate | 1, 2, 3 | 10, then 60 days | invariants on | n/a |
| Targeted studies | gate | 1, 2, 3 | 10 | none | yes where the baseline can run them (the formation, youth and loan studies read only public API) |
| Performance | gate | 1 (30 seasons), 1, 2, 3 (10 seasons) | | `/usr/bin/time -l`, invariants off | yes |

Every comparison below is **same seeds, different trajectories**: a different decision changes every later draw, so a paired
difference is not a controlled one. With three seeds the unit of independence is the seed, so the differences quoted as
"statistically meaningful" below are those that exceed what a count of events alone would produce, and are no more than that.

## 2. Invariants, finance and structure

| Check | Result |
| --- | --- |
| Invariants observer, gate: 10 seasons ×3 seeds, 30 seasons ×3 seeds | **clean in all six** (includes `banned-player-never-plays`) |
| Invariants observer, shipped, 30 seasons | **clean** |
| Clubs in debt, any season | **0 / 0 / 0** (gate 10, gate 30, shipped 30) |
| Club-seasons ending over 110 % of the wage budget | gate 10: 0 (baseline 0); gate 30: **1 of 7,110** (baseline **8**); shipped 30: 9 of 13,170 (0.07 %) |
| Median wage / budget | 1.00 in every season |
| Squads (seniors) | gate: mean 22.4 → 23.6, min 18-21, max 29-32, no thin squad, no club without a keeper; shipped: mean 22.4 → 23.6, min 19-20, max 29-32 |
| Pyramid | gate: 5 leagues; shipped: 23 leagues, 24 promoted and 24 relegated every season from season 2, no unfinished league |
| Managers leaving, 30 seasons ×3 gate seeds | 3,441 (baseline 3,461): turnover did not rise |

(Over-110 % club-seasons in the shipped world were 3 in P4.5's pre-D6 30-season run and 9 here; not paired, and each has the P4.5
causes: a loan return, a squad-floor minimum-wage signing, a budget cut.)

## 3. Formations (the P4.5 debt: 100 % one formation for ever)

Gate world, per seed, final (baseline: **1 formation, 100 %, 0 changes in every season of every seed**):

| Season | Distinct formations | Largest share | Changes in the season (a seed) |
| --- | --- | --- | --- |
| 1 | 6.0 | 57 % | 88 (1.1 per club: the opening world settling, 95 % within a month of a "new" manager) |
| 5 | 5.7 | 49 % | 21 |
| 10 | 6.0 | 43 % | 10 |
| 15 | 6.3 | 40 % | 10 |
| 20 | 6.3 | 39 % | 5 |
| 25 | 6.3 | 42 % | 4 |
| 30 | 6.7 | 38 % | 3 (0.04 per club) |

Shipped world: 7 formations at season 1 (largest 64 %, 462 changes for 439 clubs), 7-8 in seasons 2-30, largest share 64 % → 42 %,
25 changes in season 30.

Over ten seasons (three seeds, 2,370 club-seasons, `p47_formations`): 0.27 changes per club-season (about 0.2 after the first
season); 1-4 clubs a season change more than twice; 60 of 642 changes (9.3 %) are a return to the shape just left within 60 days;
63 % of changes come within 35 days of a manager new in post; the squad model's score for the shape played is 0.01-0.04 rating
points short of the best shape offered (suitability); 3-5-2 and 5-3-2 clubs beat their strength by +0.08 and +0.05 points a game
(selection: the controlled experiment with the formation assigned at random showed no effect, see the main report). 4-3-3 is the
commonest shape throughout; the squad model rates it best for about 60 % of opening squads (the corrected wingers).

**Late-career stability:** by season 30 a club changes shape about once in 25 seasons. That is stable, not frozen (six or seven
shapes in use, the same share), and the changes that remain are the managers' arrivals and squads ageing out of a shape.

## 4. Squad quality (ten seasons, gate, mean of three seeds, **RUN**)

| | Baseline | Final |
| --- | --- | --- |
| XI fit, season 1 / 3 / 5 / 10 | 68.13 / 68.63 / 69.23 / 70.60 | 69.13 / 69.53 / 70.10 / 71.50 |
| Clubs with a weak starting slot | 19.7 / 13.3 / 11.0 / 1.7 % | 8.7 / 3.7 / 4.3 / 0.3 % |
| Seniors | 22.0 / 24.6 / 24.1 / 23.3 | 22.4 / 24.9 / 24.5 / 23.4 |
| Usable bench | 8.9 / 11.9 / 12.0 / 11.6 | 8.8 / 11.8 / 12.1 / 11.8 |
| Players joining a club a season | 1.30 / 3.67 / 3.53 / 2.50 | 1.30 / 3.33 / 3.40 / 2.07 |
| Unused joiners | 7.0 / 13.3 / 14.3 / 24.7 % | 6.7 / 15.3 / 15.7 / 16.3 % |
| Transfer fees a club (M) | 0.04 / 0.93 / 1.35 / 1.57 | 0.04 / 0.81 / 1.29 / 1.09 |

The first two rows are the real gain and come from the formation review (they are there at season 1, before the market has
moved). Unused joiners at season 10 fell 8 points; fees and joiners are lower in later seasons (the strategy wiring and the
loan bar; three seeds cannot separate rule from draw).

## 5. Youth

### 5.1 The prospect cohort over ten seasons (three gate seeds, summed, `p47_youth_causes`)

| Season | Cohort at its end: baseline → final | Academy-role, first-eleven ready: baseline → final | Their minutes |
| --- | --- | --- | --- |
| 1 | 1,965 → 1,959 | 26 → 6 | 45 → 0 |
| 5 | 1,671 → 1,760 | 32 → 9 | 4 → 0 |
| 7 | 1,403 → 1,475 | 58 → 16 | 14 → 1 |
| 10 | 1,205 → **1,195** | 56 → **31** | 14 → 0 |

(Opening cohort 2,182.) The cohort has not grown (an intermediate run on the same seeds read 1,320 at season 10, so ±10 % is
noise). The specific defect is smaller by half to four fifths; it is not gone, because "ready" by group rating is often fifth-choice
in a specific slot.

### 5.2 The career-study measure, thirty gate seasons (mean per seed; baseline → final)

| Season | Prospects (n) | Prospect minutes | Share under 450 |
| --- | --- | --- | --- |
| 1 | 727 → 727 | 460 → 460 | 80 → 80 % |
| 5 | 591 → 610 | 761 → 788 | 74 → 73 % |
| 10 | 426 → 428 | 351 → 408 | 85 → 85 % |
| 15 | 387 → 408 | 400 → 410 | 86 → 86 % |
| 20 | 385 → 393 | 352 → 369 | 86 → 87 % |
| 25 | 415 → 432 | 456 → 418 | 82 → 85 % |
| 30 | 385 → 389 | 339 → 255 | 88 → 90 % |

Cohort size and the share under 450 minutes are **unchanged**; minutes at season 30 are lower (339 → 255, within the seed-to-seed
spread of this measure, which is large). **This report does not claim the prospect cohort or its minutes improved.** The P4.5
numbers (gate 770 → 387, 860 → 306 minutes) were on the pre-D6 world and are not comparable; against the paired baseline the
long-career shape (cohort halving over ten seasons then flat, minutes falling) is the same.

### 5.3 Shipped world, 30 seasons (440 clubs, one seed)

Prospects 4,055 → 2,241 (season 30); growth +2.1 (season 1, partial) then +5.0 to +5.4; minutes 359 → 301; share under 450 minutes
85 % → 89 %. P4.5's pre-D6 figures: 4,328 → 2,202, 437 → 297, 82 % → 88 %. **No improvement on the shipped world's cohort or
minutes**; no paired baseline.

## 6. Loans (the P4.5 finding: idleness rises over a career)

### 6.1 Ten seasons, three gate seeds, 60+ day loans (`p47_loans`)

| Season the loan started | 1-3 | 4-6 | 7-9 | 10 |
| --- | --- | --- | --- | --- |
| Loans: baseline → final | 305 → 223 | 620 → 456 | 529 → 404 | 318 → 178 |
| Idle (under 180 minutes) | 21 → 12 % | 30 → 16 % | 40 → 22 % | 41 → 26 % |
| Minutes per loan day | 8.64 → 10.13 | 7.31 → 9.81 | 5.97 → 8.57 | 5.66 → 8.04 |
| Arrived behind the borrower's starters | 49 % → 34 % | 64 % → 49 % | 69 % → 54 % | 71 % → 52 % |

Total minutes played on loan 12,125 → 11,625 units (−4 %). By arrival: a loanee who arrived as a starter was idle 1-9 % of the
time in both trees; one who arrived behind the starters 43-57 % in the baseline and 24-50 % in the final (the bar stops accepting
marginal starters; it does not make a bad placement good).

### 6.2 Thirty seasons, paired (career-study loan columns)

| Season | Loans ended (idle), baseline → final | Loans for minutes idle | Other loans idle |
| --- | --- | --- | --- |
| 1 | 69 (9) → 53 (4) | 35 → 25 % | 12 → 6 % |
| 5 | 71 (24) → 49 (10) | 39 → 24 % | 31 → 18 % |
| 10 | 49 (24) → 28 (7) | 41 → 33 % | 51 → 23 % |
| 15 | 55 (29) → 29 (8) | 50 → 25 % | 53 → 27 % |
| 20 | 74 (34) → 31 (12) | 55 → 20 % | 45 → 42 % |
| 25 | 66 (31) → 36 (14) | 55 → 39 % | 44 → 39 % |
| 30 | 62 (38) → 30 (17) | 73 → 44 % | 59 → 57 % |
| **Seasons 11-30, pooled** | | **293/574 (51 %) → 108/325 (33 %)** | **1,524/3,095 (49 %) → 592/1,700 (35 %)** |

The improvement is real on average (14-18 points, about 45 % fewer loans) and **narrows with career length**: by season 30 the
ordinary-loan idle share is back to the baseline's. The loan market's long-run problem is structural (mature squads need
depth, not starters, so most loans arrive behind the starters), and a higher bar thins it without removing it. The shipped world
shows the same shape: seasons 11-30 pooled, long loans idle 306/610 (50 %) for loans for minutes and 1,785/3,401 (52 %) for the
others (P4.5's pre-D6 shipped season 30: 77 % and 75 %; not paired).

## 7. Appointments (**RUN**, three gate seeds, ten seasons)

1,018 appointments. Misfit of the manager appointed 0.142 against 0.289 when choosing on stature alone (the pool's mean 0.31-0.33;
the difference against the pool is −0.183, standard error 0.004). Completed tenures 1,053, mean 380 days, median 167 (stature only:
1,007, 395 and 187). Tenure did not improve; leaving managers over 30 seed-seasons are unchanged (3,441 against 3,461).

## 8. Runtime and memory (**RUN**, instructions retired)

| Run | Baseline | Final | Change |
| --- | --- | --- | --- |
| Gate, 3 seeds × 10 seasons, invariants off | 2,436.2 B, 195.5 MB | 2,547.5 B, 190.3 MB | **+4.6 %**, −2.7 % memory |
| Gate, 1 seed × 30 seasons, invariants off | 2,496.1 B, 707 MB | 2,553.4 B, 661 MB | **+2.3 %**, −6.5 % memory |
| Shipped, 1 seed × 30 seasons, invariants on | P4.5: 23,372 B, 1,177 MB (pre-D6, not paired) | 25,282 B, 986 MB | not comparable |

The cost is front-loaded (the formation reviews and the academy look when a world is new); the 30-season figure is half the
10-season percentage. The F5.5 saving (−26.9 % against F5.4) is preserved. Wall time is **not** a result: the final 30-season gate
run shared the machine with the workspace test suite and three studies and read +15 % (5.29 against 4.60 seconds a season, flat
across the career in both); the shipped run overlapped with the same load (1,938.6 s, with one 148 s season in a busy moment, against
P4.5's 1,612 s on an idle machine).

## 9. Determinism and save compatibility

| Check | Result |
| --- | --- |
| Same binary, same seed, repeated | the loan, youth and appointment studies reproduced to the digit across runs of different commits whose changes did not touch them (for example 1,018 appointments, 0.142 / 0.325, 1,053 tenures at the appointments commit and again at the final one) |
| Ten-season reload twin, three gate seeds: save through SQLite, reload through a fresh manager, play both copies 60 days on | **identical before and after on all three seeds**; job-offer clock `Some(0)`, `Some(3)`, `Some(1)` carried by the save; invariants clean. The cosmetic reload differences P4.5 listed (national-team list order, a re-inferred weak foot) are unchanged and reach no play |
| Schema | **no migration, no new serialized field**; `EXPECTED_MIGRATION_COUNT` stays 69; `Team::formation` and `squad_role` are existing fields written by the AI; the wage reservation is derived from saved offers (tested through a JSON round trip) |
| Old saves | every AI club in a pre-P4.7 save holds the 4-4-2 it was built with and reviews it like any other; the formation, promotion and renewal rules read only persisted state |
| Pure functions | the strategy read for the day's reviewing clubs is taken once at the start of the day (a reviewer found it depended on whether an unrelated review had run first; fixed) |

## 10. Limits of this evidence

1. **Three seeds** (one on the shipped world). Same seeds, different trajectories: a paired difference is not a controlled one.
2. **No paired shipped baseline** over 30 seasons.
3. **The engine barely prices formation** (a point of squad-model deficit costs 0.005 points a game): the formation gains here are
   gains in the squad model's terms (XI fit, weak slots), not in results. Results by formation show no formation beyond strength.
4. **Prospect cohort and minutes did not move**; the cause (intake of 1-3 a club, players maturing out of the cohort) was left as
   designed.
5. **Appointment outcomes** were not tied to results.
6. **Standard errors printed by the studies ignore repeated clubs**; they are a scale, not a confidence interval.
7. **A latent defect, not mine:** a starter who retires leaves the user's saved eleven at ten until the next weekly lineup run
   (found because a changed trajectory hit `delegated_career`; not fixed).
8. **No person has played the window**: the GUI condition from P2.5 and P3 stands.

## 11. Reproduce

```sh
# from src-tauri/, release profile; the baseline in its own worktree and target directory (git worktree add --detach <dir> b5430826)
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_long_career  -- --ignored --nocapture
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=30 cargo test --release -p season-harness --test p45_long_career p45_long_career  -- --ignored --nocapture
P45_WORLD=shipped P45_SEEDS=1     P45_SEASONS=30 cargo test --release -p season-harness --test p45_long_career p45_long_career  -- --ignored --nocapture   # ~30 min
P45_WORLD=gate    P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_reload_twin  -- --ignored --nocapture
for t in p47_formations p47_formation_engine p47_formation_probe p47_youth_causes p47_loans p47_appointments p47_pricing p47_profile_cost; do
  P47_SEEDS=1,2,3 P47_SEASONS=10 cargo test --release -p season-harness --test $t -- --ignored --nocapture
done
# instructions and memory: run a built test binary under /usr/bin/time -l (copy it out of the shared target directory only
# once built: a binary copied before the build finishes is rebuilt in place), baseline built in its own worktree
```
