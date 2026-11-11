# APFM Run F4.2: football intelligence, causal tactics and gameplay validation

Branches: OFM `claude/run-f4-2-football-intelligence` (from the F4.1 report head `e76cbdce`), M12
`f4.2/football-intelligence` (from the F4.1 head `4057c67`). Nothing was merged into `apfm/main`, `develop` or an
integration branch. No save and no FM24 world was modified: every test builds its career in a temporary
directory, and the FM24 world file was only read (`apfm-fm24-2026.json`, read-only). The M12 licence edits that
were already uncommitted (`Cargo.toml`, untracked `LICENSE` / `NOTICE`) and the viewer's were left alone and are in
no commit. The session was interrupted once (the editor window was closed); it was resumed from the repositories,
the logs and the committed outputs, and no valid study was repeated unless a later change invalidated it (the
F4.2-side studies were repeated once on the final configuration for that reason; §6).

Companion document: [`APFM_F42_CAUSAL_AUDIT.md`](APFM_F42_CAUSAL_AUDIT.md) (mechanism-by-mechanism trace of one
possession). Raw outputs of every study: `docs/f42/` in the M12 repository.

## 1. Executive verdict

**Done and demonstrated**

* The football decision pipeline was traced in code and in controlled situations, and **eight specific defects** were
  found in it (§2). All eight are addressed; the dead compact block (D5) only for defensive mentalities: no pass into space, runs clamped to the last defender, pressing the passer instead of the
  receiver, pressure blind to closing speed, a shot that never beats a carry, the compact block off in production, an own goal counted as the scorer's goal, a projection that zeroed every player's cards.
* A staged counter-attack now punishes a high line: shots within 12 s are **11.2 % / 11.8 % / 22.6 %** against a line at 28 / 38 / 48 m
  (before: 6.4 / 3.2 / 3.6 %, no ordering). More attackers against fewer defenders now means monotonically more
  chances and forward passes (11 v 11 / 10 / 9: 8.3 / 10.0 / 12.0 % and 0.15 / 0.68 / 0.71 forward passes).
* The attacking mentality is **not universally safer**: against an ordinary opponent of the same level it concedes no more and
  suppresses chances; against an opponent built to beat a press and counter, the same approach concedes **+3.0 shots, +0.37 counter shots,
  +0.33 goals** and loses points (§3, issue 1).
* Own goals arise from the engine's own last-touch rule (0.034 per match, 51 in 1,500 matches), are credited to the other side, name the
  player, and the totals reconcile. Two real defects in that chain were found and fixed (§4).
* Red cards: the "0 in 71 matches" was **caused by this run's own mechanisms** (they halved fouls and removed 95 % of cautions); the referee was
  recalibrated against a sourced reference and a real suspension rule now exists (§5). A projection defect that zeroed every player's cards and
  fouls was found and fixed.
* Fatigue was re-measured on FM24 club pairs and the F4.1 neutral workload was found to be wrong by line; corrected (§9).
* A full season (38 interactive matches) was played through the application layer with a mid-season save and reload (§9).

**Not done, or only partly. Read this before trusting the rest**

* **Gate C (pressing) is not met.** Pressure now arrives twice as fast at a high press (10.0 s to 5.0 s) for 46 % more running, but
  I could not demonstrate that the pressed side makes more errors: the builder's turnover rate is flat (13 to 16 %) at every pressing level.
* **A new imbalance appeared.** On the dominance grid, defensive settings (with either pressing) beat *balanced* against all three
  opponent types (+0.2 to +0.5 points per match). The compact block, gated to defensive mentalities, is too good. Unresolved (§6.5).
* **Gate B (defensive protection) is only partly met**: `defensive` concedes 5.5 more shots but 0.30 fewer goals (CI [-0.46, -0.15]); but protecting a
  1-0 lead at 70' changes nothing measurable (§7).
* **The attacking-mentality symptom (issue 1) is partly resolved.** It is context-dependent now, but against equal opponents attacking
  still suppresses shots and goals conceded, and a 2.5x larger mentality effect makes it safer, not riskier (§3).
* Discipline is better but still below football: cautions 2.3 per match (reference 4.3 to 4.5), fouls 10.3 (about half), and almost all
  dismissals are second yellows (§5).
* Match volume moved: goals per team 1.77 (M12: 1.38), shots 15.5 (14.1). The Premier League's 2023/24 average was 1.64 goals per team (1,246 goals, [Premier League records](https://www.premierleague.com/en/news/4029394)), so this is in
  range; but it is a real change of the engine's scoring level that the career's economy was not retuned for.
* The desktop application was **not launched**; nothing here is a visual validation (§16).

| Gate | Status | Evidence |
|---|---|---|
| A. Mechanism: the opponent can exploit available space | **Met in controlled transitions**; in whole matches only through the aggregate shot and goal counts | §2, §3 |
| B. A compact organised defence restricts dangerous penetration | **Partly**: fewer goals (-0.30), more but poorer shots; no effect when protecting a lead; staged block not clearly better | §6, §7 |
| C. Pressing has advantages and costs | **Not met**: cost and arrival time shown; forced errors not | §8 |
| D. Mentality changes future behaviour without rewriting the past | **Met** (F4.1 tests unchanged and green) | §12, §14 |
| E. Numerical strength | **Partly**: monotone chances for the attack; a red card costs 1.1 chances conceded and 0.76 shots, points not resolved | §10 |
| F. Career consequences carry forward | **Met** (injuries, suspensions, card records, per-line fatigue, season, reload) | §9, §13 |
| G. Determinism and segmentation equivalence | **Met** (identical worlds and events, tests) | §12, §14 |
| H. Unusual events no longer prevent finalisation | **Met for the tested seeds** (own goals, woodwork, dismissals, injuries over hundreds of natural matches) | §13 |
| I. Performance | **Met**: 0.80 s a match (+24 %) | §15 |
| J. Playability | **Application level only** (interactive match saved, reloaded, finalised; a season); GUI not launched | §16 |

## 2. Root causes found in the code (and what was done)

Details and file references are in the audit. Evidence: staged situations, `inspect_pass_options_detailed`, a read-only decision trace
(`FootballRuntime::last_decision`), and the duel-to-referee funnel.

| # | Defect | Demonstrated by | Change (switch in `M12Config`) |
|---|---|---|---|
| D1 | A runner level with the line is offered the ball at his feet only; the space behind him is not a destination | Pass options at +0.9 s: best option 1.03 (short), nothing beyond | `space_passing`: through-ball variants, destination danger, receiver race, loss exposure |
| D4 | `stay_onside` clamps every run to the last defender | code; runs never beyond the line | `run_beyond_m`: runs may pass the line while the carrier is free |
| D6 | While a pass is in flight the press targets the **passer** | Press trace: pressers on the passer 1 to 3 s after release | `press_receiver` |
| D7 | Pressure is distance-only; a hurried pass is not executed worse | code | `pressure_decisions` (closing speed; composure-weighted execution) |
| F1 | **A free carrier never shoots**: shot utility `3.2 q - 1.3` never beats a carry (+0.45 to +0.75), so a striker through on goal dribbles to the by-line | Decision trace: at 7 m from goal, quality 0.28, shoot -0.41, carry +0.45; box reached in 80 to 93 % of counters, a shot in 1 to 2 % | `finish_min_quality`: a shot of quality >= 0.25 replaces the carry when not pressed |
| D5 | `RecoverGoalside` and the compact block are **dead code in production** (`m11.defence.shape` off) | label counts in a full match: 0 | `block_mentality_max`: a defensive mentality defends in the block |
| X1 | Own-goal scorer credited with a **goal** in the engine's player statistics | `f42_own_goals` red before the fix | `own_goals` statistic |
| X2 | Projection writes **zero** cautions, dismissals and fouls on every player | `a_players_cards_and_fouls_reach_his_own_record` red before the fix | `record_discipline` |

Not changed and still true: the engine's shot distribution (mean 10.3 m, 65 % on target) is a pre-existing calibration; recovery is a function of
distance to the anchor, not of the threat (D5 in the audit), so **checkpoint 4 (defensive transitions) is only partly done**: the label that
describes a recovery is now live for defensive teams, individuals recover through the engine's physical limits, and nothing teleports, but no
threat-driven recovery point was implemented.

### Before and after, staged situations (300 to 500 paired seeds each)

| Situation | Before (M12) | After (F4.2) |
|---|---|---|
| A. counter v line 28 / 38 / 48 m, shots in 12 s | 6.4 / 3.2 / 3.6 % | **11.2 / 11.8 / 22.6 %** |
| E. same attack v 11 / 10 / 9, shots in 20 s | 10.3 / 12.7 / 11.3 % | **8.3 / 10.0 / 12.0 %** |
| E. forward passes in 20 s | 0.18 / 1.10 / 1.03 | 0.15 / 0.68 / 0.71 |
| D. press beaten, 0 / 4 / 6 pressers beyond the ball, shots in 16 s | 4.3 / 4.0 / 11.0 % | 4.7 / **13.0 / 13.0 %** |
| C. pressure reaches the ball, press 10 / 50 / 85 / 100 (s) | 12.0 / 9.0 / 9.4 / 9.5 | **10.0 / 7.2 / 5.3 / 5.0** |
| C. pressers' high-speed running (m) | 758 / 914 / 1079 / 1082 | 754 / 958 / 1108 / 1092 |
| C. builder loses the ball within 20 s | 10.7 / 15.7 / 11.7 / 13.0 % | 14.0 / 13.0 / 11.7 / 16.3 % (**flat**) |
| B. block of width 0.75 v standard width (same depths), shots in 20 s | 19.7 v 10.0 % | 12.7 v 9.3 % (**the narrower block is not better**: it leaves the flanks) |

## 3. Issue 1: the attacking mentality still reduces shots conceded

**A. Symptom.** F4.1 and the first F4.2 ladder: `very_attacking` concedes fewer shots than `balanced`, and the counter-attack shots conceded
never rise.

**B. Root cause demonstrated.** Four separate findings, in the order they were found:

1. The *attack side* could not exploit exposure at all (F1, D1, D4): in staged counters the ball reached the box in 80 to 93 % of 12-second sequences and
   a shot followed in 1 to 2 % (trace in §2). So even an exposed team was never punished; this was a defect independent of mentality.
2. After the fix, exposure is punished when it is large (Scenario A: 22.6 % against a 48 m line, twice the 28 m line).
3. But the exposure a *mentality* creates is small: the very attacking back line stands 2.6 m higher (Table below, `back line depth`), against the 20 m of the
   staged contrast.
4. And the effect that remains is **territory**, not possession: possession share is unchanged (0.491 v 0.500), but the very attacking team wins the ball
   in the opponent's final 40 m **+2.1 times a match** (CI [+1.7, +2.5]), which cuts the opponent's possessions at the source.

Dose-response (`docs/f42/dose_commit2.5_ladder.txt`): multiplying the F4.1 mentality commitment by 2.5 (back line 6.6 m higher) makes the very attacking team
**safer**, not riskier (shots conceded -2.39 against -1.13; counter shots -0.16 [-0.32, -0.00]; points +0.46). So the cause is not "the mentality
moves the lines too little".

**C. Football mechanism that should exist.** A team that commits men is suppressed-by-territory against an ordinary opponent, and exposed to one that can beat
the press and attack the space: the value of attacking depends on the opponent.

**D. Implementation.** Finishing (`finish_min_quality`), space-aware passing, runs beyond the line, pressing the receiver (§2). No multiplier on shots or goals.
The opponent-context study (`tests/f42_context.rs`) is the validation instrument.

**E. Validation** (500 paired seeds for the ladder, 400 for the context study; the opponent has the same overall level, only its profile differs):

| `very_attacking` (+ high press) v balanced | ordinary opponent | press-resistant counter opponent |
|---|---|---|
| shots conceded | +0.35 [-0.18, +0.87] | **+3.02 [+2.32, +3.72]** |
| open chances conceded | -2.49 [-2.92, -2.05] | -4.32 [-4.92, -3.72] |
| counter-attack shots conceded | +0.17 [-0.03, +0.36] | **+0.37 [+0.11, +0.63]** |
| goals against | -0.26 [-0.44, -0.08] | **+0.33 [+0.09, +0.58]** |
| points | +0.07 [-0.12, +0.26] | **-0.14 [-0.27, -0.01]** |

Ladder against an identical balanced opponent (final F4.2 configuration, 500 pairs, mean paired difference, 95 % CI):

| | very defensive | defensive | attacking | very attacking |
|---|---:|---:|---:|---:|
| shots for | -1.36 | -0.46 | +1.22 | +1.81 |
| shots conceded | **+6.93** | **+5.55** | -0.46 [-0.93, +0.00] | **-1.13 [-1.60, -0.66]** |
| open chances conceded | +4.30 | +3.42 | -0.29 | -0.92 |
| counter-attack shots conceded | +0.84 | +0.70 | +0.04 | -0.03 |
| high recoveries (won in opp. final 40 m) | -0.98 | -0.34 | +0.43 | **+2.12** |
| goals against | -0.18 | **-0.30 [-0.46, -0.15]** | -0.10 | -0.18 [-0.34, -0.02] |
| points | +0.03 | +0.09 | **+0.18 [+0.01, +0.34]** | **+0.23 [+0.07, +0.39]** |

(M12 baseline, same seeds: very attacking shots conceded -1.15, goals against -0.02, points +0.14; attacking -0.82 / -0.05 / +0.13.)

**F. Remaining limitations (unresolved).**
* Against **equal** opponents the attacking mentality remains safer in shots and open chances, and it gains points. Real football would give the dominant
  side fewer chances against too; but "not universally safer" is demonstrated only against a specifically built opponent, and I did not test which
  real-world opponent mix a career produces.
* Counter-attack shots conceded do not rise with the attacking mentality (1.66 to 1.62 per match): the exposure shows up in the staged transition and in the
  specialised opponent, not as a general counter-attack rate.
* The "same level" counter opponent is also stronger overall in attack (2.8 goals against the balanced reference), so the points effect (-0.14) mixes the
  risk of attacking with a bad matchup. The shots and counter-shot effects are the cleaner evidence.

## 4. Issue 2: own goals

**A. Symptom.** F4.1 projected an own goal without a scorer; nobody verified that natural own goals occur, who is credited, or what the record says.

**B. Pipeline audit.**
1. *Does M12 generate them naturally?* Yes. `rules.rs` credits a line crossing to the attacking side and marks it `own_goal` when the last toucher plays for the
   other side. **0.034 per match** (51 in 1,500; 1.0 to 1.3 % of goals) in both configurations. Preceding events (4 s window): shot deflected or blocked in 31 %
   (baseline) / 57 % (F4.2); pass or cross turned in 25 % / 14 %; no recent action (clearance, loose ball) 41 % / 24 %; the goalkeeper touched it first 2 % / 4 %.
   Scorers: 98 % / 90 % outfield. Nothing is scheduled or fabricated. `examples/f42_owngoals.rs` reproduces it.
2. *Realism of the rate.* I could not source a reliable reference: the one table found ([Extratime, PL 2023/24](https://www.extratime.com/stats/2127/2287/premier-league/)) lists 49 and looks
   mislabelled. The rate is plausible to low (a few percent of goals); **unverified**.
3. *OFM decoding*: correct since F4.1 (the goal counts for the beneficiary, no `GoalDetail`).
4. *Career record*: three defects found by `a_natural_own_goal_is_credited_to_the_other_side_and_names_its_player` and the unit tests:
   * **X1** the engine's player statistics gave the scorer of an own goal a *goal* (M12 `stats.rs`): his season goals, his rating and the "goals credited to
     players" total were wrong. Fixed in the authoritative layer: `PlayerFootballStats.own_goals`; the goal-consistency check now counts own goals separately.
   * the timeline named nobody (F4.1) and the stored match history (the compact record kept on the fixture) could not name him: now `EventDetail::OwnGoal { by }`
     in the report and `CompactMatchEvent.own_goal_by` (defaulted: old saves load) in the history.
   * the feeds showed a goal with no name (live feed, half-time list, post-match list, scorer list, round digest): one `goalPlayerLabel` and the existing translated
     key `match.replayFeed.ownGoal` ("Own goal: <player>") in all 12 locales.

**C. Mechanism.** An own goal is an event of play (last touch) credited to the other side, naming the player only when the engine knows him; distinct from goals and assists.

**D. Implementation.** `stats.rs`, `spatial_match_projection.rs` (`OwnGoal` detail), `post_match.rs` (`own_goal_by`), `helpers.tsx` / feeds.

**E. Validation.** `f42_own_goals` (M12, three natural own-goal matches: goals + own goals = score, scorer has an own goal and no goal);
`a_natural_own_goal_...` (OFM interactive, seed 12: `away-p5` puts it in, home is credited, his club is the conceding one, the history names him, totals reconcile,
nobody is a scorer); serialisation contract test; frontend tests (celebration commentary suppressed, scorer list, event feed). The old projection test now
asserts the player.

**F. Remaining limitations.** No per-player *season* own-goal tally in OFM (it would touch ~60 struct literals; the history and timeline name the player);
the rate is 1.0 to 1.3 % of goals; no refinement of "dubious goal" attribution (a shot already going in that a defender merely touches is an own goal).

## 5. Issue 3: zero red cards in 71 matches

**A. Symptom.** 0 dismissals in 71 OFM matches.

**B. Root cause demonstrated.** Two causes, one of them mine.

* **Caused by F4.2.** 1,000 matches, M12 against the first F4.2 configuration: fouls 10.8 to 4.8, cautions 0.86 to 0.17, dismissals **0.042 to 0.003**. The duel funnel shows
  the same number of challenges (122 judged per match) but a lower **closing speed at contact: 1.87 to 1.41 m/s**, and the foul severity only counts closing speed above 2.5 m/s.
  `press_receiver` (7.7 fouls) and `pressure_decisions` (7.1) each lower it; pressers now meet the receiver instead of running at him from behind.
* **Pre-existing.** Even unmodified M12 gives 0.042 dismissals a match [0.025, 0.058] (P(none in 71) = 5 %), 10.8 fouls and 0.86 cautions: about a quarter of football's
  rate. The lab's own history: 0.58 dismissals (M8), 0.06 (M9), "cannot be judged without reference data" (`DUEL_AND_FOUL_AUDIT.md`).

**Reference.** Premier League 2023/24: about 0.16 red cards and 4.3 to 4.5 yellow cards per match ([The Analyst](https://theanalyst.com/eu/2023/12/premier-league-record-breaking-red-card-season/),
[Goal.com](https://www.goal.com/en-gb/betting/premier-league-cards-betting/bltd650e84c266fcf43)); 62 to 64 sendings-off a season in the decade before (The Analyst). Fouls per match: the sources found
give team figures only (the highest, Bournemouth, 13.4 per match), so a league mean of about 20 to 24 is **my estimate, unsourced**. Era and competition differ from the game's; this is a
range, not a target to hit.

**C. Mechanism.** Cards come from the referee's reading of real challenges (closing speed, approach from behind, ball played, aggression, tackling) against thresholds. The thresholds are the
referee's strictness: the part of the model that can legitimately be calibrated against football.

**D. Implementation.** `full_match_f42()` sets `careless 0.6 / reckless 1.1 / excessive 1.9` (from 0.9 / 1.7 / 2.3). The physics is unchanged; no event is inserted anywhere. Suspensions:
`ofm_core::suspension` (§13). **X2**: the projection wrote zero cautions, dismissals and fouls on every player; now taken from the timeline's own events.

**E. Statistical validation** (1,000 matches, F4.2 configuration, 95 % CI):

| per match | M12 | F4.2 before recalibration | **F4.2 final** | reference |
|---|---:|---:|---:|---|
| fouls | 10.77 [10.47, 11.06] | 4.80 | **10.30 [10.07, 10.53]** | ~20 to 24 (estimate) |
| cautions (incl. second yellows) | 0.86 [0.78, 0.93] | 0.17 | **2.29 [2.19, 2.39]** | 4.3 to 4.5 |
| second yellows | 0.022 | 0.003 | 0.120 [0.097, 0.143] | |
| straight reds | 0.020 | 0.000 | 0.042 [0.029, 0.055] | |
| **dismissals** | 0.042 [0.025, 0.058] | 0.003 | **0.162 [0.136, 0.188]** | **~0.1 to 0.17** |
| penalties awarded | 0.33 | 0.19 | 0.48 [0.44, 0.53] | ~0.3 (unsourced) |

On FM24 club pairs (120 matches, default pressing, 40 matches: wide intervals): 13.2 fouls, 3.55 cautions, 0.23 dismissals (all second yellows).

Consequences of a dismissal, each tested:

| required | evidence |
|---|---|
| removes the player | `f42_dismissal`: his slot is in `world.dismissed`, his body parked off the pitch, his side at most ten |
| prevents further participation | no event of the rest of the match names him |
| appears in the report | card event; team statistics count it; the projection writes it on his record (`a_players_cards_and_fouls_reach_his_own_record`) |
| changes spatial structure and future football | ten men: -0.76 shots, +1.09 shots and +1.00 open chances conceded (Scenario 3, 500 pairs); staged 11 v 10 / 9 (§10) |
| produces a suspension in the career | 3 matches for a straight red, 1 for a second yellow; set from the report's card events, counted down by the club's matches, excluded from squad selection; `injuries_and_dismissals_...` reaches it from a natural dismissal end to end |

**F. Remaining limitations (unresolved).** Cautions are half of football's; fouls are half; penalties are high (fouls concentrate in the box); **almost every dismissal is a second yellow**
(straight reds 26 %, football is the reverse) because a single severity scale cannot raise straight reds without also raising cautions; the thresholds were fitted on the engine's fixture teams and
checked, not fitted, on FM24 players; the engine judges only challenges on the ball carrier (no holding, no off-the-ball fouls), which is why a foul count of football's size is out of reach by thresholds alone.
The ban lengths (3 / 1) are the usual minimum of the main leagues, not a competition rule set; nothing shortens, appeals or carries a ban across a transfer.

## 6. Tactical statistics (checkpoint 6)

### 6.1 Method

Paired seeds (identical teams, level 65, same seeds, only the home orders differ), 500 pairs for the principal ladder, 400 to 500 for scenarios, 300 for the grids; the instrument and the unmodified
baseline reproduce F4.1's published numbers exactly (determinism). Mechanisms were chosen and calibrated *before* the final studies on separate seed ranges and whole-match volumes
(shots, goals, passes, completion, fouls); no study was chosen for its result, but the F4.2 side was run on the final configuration once.

### 6.2 Ladder
See §3. The **structural** effects are the F4.1 ones and unchanged (mid line depth with ball +5.5 m, goal-side players after a loss -0.31).

### 6.3 Mentality x pressing (300 pairs, `final_f42_mentality_and_pressing...`)
`attacking + high press` v balanced: open chances conceded **-2.85 [-3.38, -2.32]**; shots conceded -0.10 (ns); possession lost in the team's own final 40 m **+1.52 [+1.00, +2.05]** (the cost);
counter-attack shots conceded +0.16 (ns); energy and pass volume up (+18.5 passes). The press pays in chances and costs in own-third losses and energy; it does not pay in forced turnovers (§8).

### 6.4 Quality beats tactics (300 pairs, 72 v 58)
Quality is worth **+1.35 points** against equal opposition; the whole spread of tactics is **0.12 points**. The strong team wins 87 to 91 % in every setting.

### 6.5 Dominance grid (mean points per match for the home team, 300 seeds a cell)

| home setting | v balanced | v attacking + high press | v defensive + low block |
|---|---:|---:|---:|
| balanced | 1.45 | 1.40 | 1.20 |
| very defensive | 1.49 | 1.16 | 1.24 |
| defensive | 1.50 | 1.54 | 1.33 |
| attacking | 1.57 | 1.46 | 1.32 |
| very attacking | 1.55 | 1.31 | 1.32 |
| attacking + high press | 1.33 | 1.22 | 1.30 |
| attacking + low press | 1.42 | 1.33 | 1.00 |
| **defensive + high press** | **1.67** | **1.59** | **1.62** |
| **defensive + low press** | **1.69** | 1.50 | **1.59** |

The test's assertion (no setting is significantly better than balanced against *every* opponent) passes cell by cell, but `defensive + high press` and `defensive + low press` are
ahead of balanced in **all three columns by 0.2 to 0.4 points** (a cell's standard error is about 0.085), and `attacking + high press` is behind it in all three. **This is a new imbalance, not a pass.**
The likely cause is the block, which is only given to defensive mentalities: it concedes poorer shots (goals against -0.30) while the finishing rule converts the team's own counters. I did not
find a setting of `block_mentality_max` that I could justify without another run; unresolved.

## 7. Scenarios (checkpoint 7)

All paired, same seeds where the state arose naturally; 95 % CI.

1. **Protect a 1-0 lead at 70'** (402 seeds). F4.1: protecting made things worse (-0.235 points). Now, decomposed: block only: shots conceded +1.35 [+1.10, +1.60], open chances +0.80, goals against +0.02 [-0.06, +0.09],
   points -0.05 [-0.16, +0.07]; less pressing only: shots +0.02, counters conceded -0.10 [-0.18, -0.02], points -0.01; both: win 82 % v 82 %. **No measurable benefit, and no harm.** Dropping the press and
   dropping the line are different decisions, and neither alone protects a lead here.
2. **Chase 0-1 at 70'** (379 seeds). Going for it: shots conceded **-0.29 [-0.54, -0.04]**, goals unchanged, points +0.03 [-0.05, +0.12]. The attacking approach creates no more chances in 20 minutes.
3. **Red card at 35'** (500 pairs). Ten men: shots for **-0.76 [-1.10, -0.43]**, shots conceded **+1.09 [+0.73, +1.46]**, open chances conceded **+1.00 [+0.68, +1.33]**, goals against +0.10 (ns), points -0.09 [-0.22, +0.04].
   Reorganising (4-5-1): goals against **-0.17 [-0.29, -0.05]** but shots conceded +2.48, points +0.10 (ns). The disadvantage is real in chances; its effect on points is **not resolved** at this sample size.
4. **Fresh substitutes after 70' of high pressing** (500 pairs). Condition at the end +7.2 points; shots no different; points **+0.094 [+0.005, +0.183]**. Freshness alone does not move chances; the points effect is small.
5. **Strong v weak (72 v 58)**: §6.4. A realistic gap (14 attribute levels) is still large; a league-typical gap was not run.
6. **A season of consecutive fixtures** (done, §9).

## 8. Pressing (checkpoint 5)

* **Trigger to approach** (fixed, D6): pressers now go to the receiver of a pass in flight. Pressure arrives at the ball at 10.0 / 7.2 / 5.3 / 5.0 s for pressing 10 / 50 / 85 / 100 (was 12.0 / 9.0 / 9.4 / 9.5).
* **Pressure on the carrier** (D7): closing speed counts (0.3 s look-ahead) and a hurried pass loses up to 15 % of execution quality, less for a composed player. These were calibrated against whole-match volumes: stronger values
  (0.6 s, 30 %) cut pass attempts by 20 % and forward passes by 28 %, which is not football.
* **Cost** is real: 758 to 1,108 m of high-speed running for the pressing side (+46 %), energy +3 % at the extremes, condition at the end -5 points for a high press.
* **Benefit not demonstrated.** The builder's turnover rate is flat; the press does not create forced errors in the staged build-up. In whole matches the press reduces open chances (-2.85) and costs own-third losses (+1.5).
  A first variant with larger pressure parameters did raise builder turnovers to 45 to 52 %, but it was the through-ball option (more risky forward passing) doing it, with passes down 20 %; I did not keep it.
* Low pressing remains legitimate (it saves energy: condition +8.5 points in F4.1) but not tested again here.

## 9. Fatigue and a full season (checkpoints 8 and scenario 6)

### 9.1 Re-measurement on FM24 players (`fm24_workload_reference_study`, 120 matches, read-only world)
F4.1's single neutral rate (0.0025 of the reserve per minute) came from synthetic players. On real club pairs it is wrong by line:

| line | n (>= 85') | rate per minute (stamina-normalised) | ratio to the F4.1 constant |
|---|---:|---:|---:|
| defender | 308 | 0.00068 | 0.27 |
| midfielder | 233 | 0.00327 | 1.31 |
| forward | 93 | 0.00422 | 1.69 |

A single rate charged every defender the **0.6x floor** and every forward the **1.5x ceiling** whatever the match. `player_wear` now compares a match with the neutral of the player's own line; the mean ratio on the same data is
**0.94 / 1.05 / 1.00** (in-sample: the constants come from these matches; no held-out set was run). Pressing is not free: raw rate by pressing (midfielders, stamina 55 to 69) passive 0.00286, default 0.00343, aggressive 0.00360.
The hybrid model, the single writer and the 0.6 to 1.5 bounds are kept. **Unresolved engine realism**: the engine spends five times more of the reserve on a forward than a defender; football's gap is a few percent.
Goalkeepers are excluded (no reference). Differences by stamina band were visible after normalisation (band >= 70 lower by 30 to 45 %), so the stamina term is probably too weak; not changed.

### 9.2 Season (`interactive_season`, 38 interactive matches of one club in a generated three-nation world, release of the application layer)
* every league fixture finalised once; the club's table row equals its completed fixtures and `standings_match_results` finds no fault;
* players who played arrived with condition **94.9 (first third) to 92.4 (last third)**; the lowest arrival 65; squad average 91.2 before a match, 75.3 after; **no spiral**;
* 12 red cards in the season were recorded and the players kept out; no injured or suspended player played;
* a save and reload after match 10 reproduces the uninterrupted season exactly (fingerprint equal: results, players, staff, inbox).
* `injuries_and_dismissals_...`: injuries of 150-match scans reach the career as injuries on the players hurt only (the club's medical staff catch some, by design).
Limitations: one world, one club, one seed; no rotation orders (the club plays its own plan); the 20 and 45 thresholds in the test are undocumented heuristics.

## 10. Numerical strength (checkpoint 7 / gate E)
Staged attack (carrier at the edge of the block, 20 s, 300 pairs) against 11 / 10 / 9: shots 8.3 / 10.0 / 12.0 %, forward passes 0.15 / 0.68 / 0.71, turnovers 41 / 61 / 55 %. Direction right, magnitude modest, and 10 to 9 men is not
distinguished. Nothing in the decision code counts players explicitly; the effect comes through lanes and reach times.

## 11. AI manager consistency (checkpoint 9)
OFM's interactive policy now ranks a replacement with M12's own `substitution_utility_v2` (quality in the slot, positional fit, fatigue-based performance, tactical term for the score and the clock, cost of a change), with M12's horizon
(extra time measured against its thirty minutes) and the gating `need` (`chase.max(protect)`). OFM keeps *when* it looks (checkpoints, windows, the allowance). Urgency was already shared (F4.1).
**Still two implementations**: formation choice after a dismissal (`formation_utility` is not used by OFM), opponent-momentum sensing, `deficit_line` and the short-term fatigue and injury handicap terms.
Tests: 20 policy tests; `a_replacement_is_the_one_the_autonomous_manager_would_choose` compares the OFM choice with an independent call of the M12 function on six states (regulation and extra time).
**Weakness found by the tests reviewer and not closed:** that parity test cannot tell the new policy from the old on its fixture (one natural centre-mid on the bench); the score and need wiring is not covered by a
discriminating test. Treat the unification as implemented but weakly guarded.

## 12. Determinism, checkpoints, compatibility (checkpoint 10 J)
A checkpoint stores the full football configuration, so every switch replays as recorded. A build also refuses a checkpoint written under a different `BEHAVIOUR_REVISION` (now 1; absent = 0; four tests). Whoever changes football
code that is *not* behind a switch must bump it; the golden suites fail on exactly such a change. Consequence: an in-progress F4.1 match is refused with the existing "match discarded" notice and restarts from kick-off.
`full_match_m12()` is untouched; OFM uses `full_match_f42()` through one function (`spatial_runner::match_football_config`). Whole-match, segmented (17/44/61') and one-minute-sliced runs are identical under the F4.2 configuration.

## 13. Correctness items of the brief (checkpoint 10)

| | Item | Result |
|---|---|---|
| A | own-goal attribution | fixed, §4 |
| B | interactive red-card suspension carry-over | **no suspension rule existed**; implemented (3 / 1 matches), migration v065, selection, badge (12 locales), tests |
| C | injury carry-over | verified end to end (injuries reach the career; none plays injured) |
| D | eligibility after suspension or injury | one predicate `Player::can_play`; the match-day pool, the user's and AI selection, the assistant, AI tactics |
| E | finalisation after woodwork, own goals, late goals, stoppages | hundreds of natural matches finalised in the scans (own goals, dismissals, injuries); the F4.1 20-seed regression test stays |
| F | save/reload with pending substitutions and tactical orders | tested (pending substitution still waiting, applied exactly once, tactical order in force) |
| G | no duplicated events after restoration | finalisation refuses a duplicated event; event cursor equal after restore |
| H | no large replay data in saves | unchanged (checkpoint 100 KB, no frames) |
| I | reproducible private dependency and CI | pinned by revision; CI composite action `private-engine-access` writes a read-only token (`APFM_ENGINE_READ_TOKEN`, **must be created as a repository secret**) into the runner's git config only; `deny.toml` allows exactly the engine repository |
| J | compatibility policy | §12 |
| K | the five reviewers | §17 |
| L | viewer and non-interactive paths | viewer: 142 tests pass; an F4.2 replay loads (validation ok, 0 errors, 1 warning not investigated); OFM instant and replay paths use the same configuration; `npm run build` ok |

## 14. Regression results

| Suite | Result |
|---|---|
| M12 `cargo test --release --no-fail-fast` (first complete run) | 602 passed, 2 failed, 20 ignored (studies). Both failures were the goal-consistency check that still assumed own goals are player goals; fixed (`98d9ead`) and `m7_match` 16/16 |
| M12 final head `f4ab176`, 14 key suites + lib | all green: f42_mechanisms 5, f42_checkpoint_policy 4, f42_own_goals, f42_dismissal, m7_match 16, f4_session_equivalence 2, f4_session_commands 15, m12_audit 6, m10_matches 9, m10_management 9, f41_mentality_shape 10, f41_restart_geometry 4, m12_defence 8, m11_defence 19, f4_tactics 10, lib 50 |
| M12 `clippy --all-targets -D warnings`, `fmt` | clean |
| OFM `cargo test --locked --workspace` against the **pinned git revision** | 4,448 passed, 0 failed, 58 ignored (the first run found 3 failing older-release db fixtures that did not know the new column; fixed) |
| OFM `--features mcp --lib` | 633 passed |
| OFM clippy default and `--features mcp`, `--locked --workspace --all-targets -D warnings`; `fmt --check` | clean |
| `npm test` | 331 files, 2,913 passed, 4 skipped, 0 failed (the style-guide test found the new primitive undrawn; fixed) |
| `npm run build`, `lint`, `format:check`, `tsc`, `knip`, `src/i18n` gates | clean |
| Ignored studies run | season (twice), FM24 workload, discipline (1,000 matches), own goals (1,500), ladder (500 pairs, both configurations), 3 scenarios, grids |
| Not run | `mcp-e2e`, mutation tests, the desktop application |

## 15. Performance (Apple Silicon, release, quiet machine, `examples/f4_perf.rs`, 10 matches)

| | M12 mode | F4.2 |
|---|---:|---:|
| whole match | 0.641 s | **0.797 s (+24 %)** |
| one-minute slices | 0.639 s (-0.3 %) | 0.798 s (+0.1 %) |
| view() + JSON | 12.6 us | 12.1 us |
| submit(tactical command) | 155 us | 154 us |
| checkpoint at 30' | 100,059 bytes | 100,056 bytes |
| restore | 240 ms | 300 ms |
| peak memory | 5.8 MB | 5.9 MB |
| season of 38 interactive matches + the world's days (debug test profile) | | 58 s |

F4 measured 0.61 s. The +24 % is the through-ball option variants (three candidates per teammate, a lane evaluation and a reach-time race each); not optimised. 0.8 s a match is about 30 s for a 38-match season.

## 16. Manual playthrough instructions and limits
The desktop application was **not launched**; nothing here is a visual validation. 1. `npm run tauri dev`, new career or a load; 2. open the fixture, **Manage the match**, play; 3. at half-time change mentality and make a substitution;
4. watch the feed for an **Own goal: <player>** line and the card events; 5. after a sending-off, finish the match and open the squad: the player carries a red **Suspended N** badge and is absent from the next XI for N matches;
6. save mid-match, quit, load: the match resumes (an F4.1 save is refused with a notice); 7. finish: the table moves once.

## 17. Reviewers (the five read-only agents of CLAUDE.md)

| Reviewer | Findings | Disposition |
|---|---|---|
| architecture | pin did not contain `full_match_f42` (blocker); new dev edge root to `season-harness`; minor behaviour change in the AI | pin moved and verified without the local override; edge recorded in `src-tauri/CLAUDE.md`; AI change documented |
| i18n | none blocking; round digest showed "Unknown" for other fixtures' own goals | fixed (`own_goal_by`); 2 new badge keys x 12 locales added later, covered by the i18n gates |
| accessibility | other goal lists showed a blank name; raw palette classes on the new line | one helper used in every list; palette left to match its neighbours (folder not yet migrated) |
| dedup | adapter drifted from M12's horizon and `need`; season test re-implemented harness checks | fixed (extra-time horizon, `need_v2`), harness `standings_match_results` and `fingerprint` used |
| tests | parity test cannot discriminate; dismissal half vacuous; `<= 1` not exactly once; EventFeed untested; contract untested | dismissal test now scans until a natural dismissal and asserts suspensions; `== 1`; feed and contract tests added; **parity test discrimination NOT fixed** |

I did not re-run the reviewers after the later changes (suspensions, discipline projection, wear, finishing). The new code since their reports has had the tests above and no independent review. Mutation checks done: own-goal frontend and the projection test fail without their fix; the
per-line wear, suspension and stored-history tests were written together with the change and not mutation-checked.

## 18. Remaining limitations (all of them)

1. Gate C: pressing creates no demonstrated forced errors. 2. Defensive settings dominate balanced (§6.5). 3. Attacking mentality is safer against equal opponents; counter-attack shots do not rise with it.
4. Defensive transitions: recovery is not threat-driven; `RecoverGoalside` is live only inside the block for defensive mentalities. 5. Protecting a lead has no measurable effect on points; ten men's cost in points is unresolved.
6. Discipline: cautions and fouls about half of football's, penalties high, dismissals almost all second yellows; the referee judges challenges on the carrier only. 7. Goals per team 1.77 (M12 1.38), shots 15.5, 65 % on target and a 10.3 m mean shot distance (pre-existing)
   are unrealistic in different directions; career economy not retuned. 8. Own goals: rate unverified, no per-player season tally. 9. Wear: constants fitted and checked on the same 120 matches; stamina term probably too weak; forwards cost five times a defender in the engine.
10. AI: formation choice and opponent momentum still separate; the parity test is weak. 11. Season: one world, one club, one seed. 12. +24 % match time. 13. The ladder and grids use equal teams at level 65; no league-typical gap. 14. No re-review of the late changes. 15. Translations of the two badge keys and nothing else were written for this run without a native review.
16. The `APFM_ENGINE_READ_TOKEN` secret does not exist yet: CI cannot build this branch until it does. 17. GUI not launched.

## 19. Git

| Repository | Branch | Base | Head |
|---|---|---|---|
| OFM | `claude/run-f4-2-football-intelligence` | `e76cbdce` | see `git log` (last commit: this report) |
| M12 | `f4.2/football-intelligence` | `4057c67` | `f4ab176daea68f280b6791be4242ea4742bd6cea` |

M12, oldest first: `4eee948` space-aware passing, runs, pressure, receiver pressing, staging primitive, behaviour revision; `6dad7e4` `full_match_f42`, the block gated by mentality; `14e6fa9` mechanism tests, diagnostics;
`a89aa20` clippy; `11a382e` finishing rule, referee calibration, own-goal statistics, discipline and own-goal studies; `98d9ead` consistency check; `54f6f5d` dose-response and context studies, perf example, final outputs; `4c33697`, `f4ab176` dismissal test.

OFM, oldest first: `c0715157` audit; `43cab712` own goal named; `a2b277de` CI access; `d9f11a57` one configuration, AI utility, season and carry-over tests; `cbf3a06a` reviewer findings; `241b3403` suspensions, discipline projection, history;
`1873ae98` db fixtures, style guide; `57461848` pin; `9f9b180e` per-line wear; then the final pin to `f4ab176`, the suspension note in `GAME_SYSTEMS.md` and this report.

## 20. Push status
M12 `f4.2/football-intelligence` is **pushed** to `origin` (private) at `f4ab176`. OFM: see the final line of the session's answer. Nothing was merged into `main`, `develop` or an integration branch.
To build OFM against the pinned revision, git access to the private M12 repository is required (a credential helper, `gh auth setup-git`, or the CI token). For local work against a sibling checkout copy
`src-tauri/.cargo/config.toml.example` to the git-ignored `src-tauri/.cargo/config.toml`.

## 21. Reproduce

```bash
# M12 (~/apfm-match-lab), flags: f42 = the F4.2 configuration; any single mechanism: space, run=3, pressure, press_receiver, finish=0.25, block=0.4
cargo test --release --test f42_diagnostics -- --ignored --nocapture scenario_a_final     # F42_FLAGS="f42" or ""
F42_FLAGS=f42 F42_SEEDS=500 cargo test --release --test f41_calibration -- --include-ignored --nocapture report_the_mentality_ladder
F42_FLAGS=f42 F42_SEEDS=400 cargo test --release --test f42_context -- --ignored --nocapture
cargo run --release --example f42_discipline -- 1000 f42      # discipline; also f42_owngoals, f42_baseline, f4_perf (F42=1)
# OFM
cargo test --locked --workspace
cargo test --locked --lib interactive_season -- --ignored --nocapture
APFM_WORLD_JSON=<apfm-fm24-2026.json> cargo test -p ofm_core --lib fm24_workload -- --ignored --nocapture
```
