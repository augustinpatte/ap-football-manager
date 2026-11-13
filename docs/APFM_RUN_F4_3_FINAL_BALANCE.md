# APFM Run F4.3: final football balance, physical realism and gameplay validation

Branches: OFM `claude/run-f4-3-final-balance` (from the F4.2 head `50291c7a`), M12 `f4.3/final-balance` (from the F4.2 head
`f4ab176daea68f280b6791be4242ea4742bd6cea`, verified before anything was changed). Nothing was merged into `apfm/main`, `develop` or an
integration branch. No save and no existing career was touched: every test builds its career in memory or in a temporary
directory, and the FM24 world was read from a **copy** placed in the session's scratch directory (the original file under
`output/` was never opened for writing). The M12 licence edits that were already uncommitted (`Cargo.toml`, untracked `LICENSE` /
`NOTICE`) and the viewer's (`package.json`, `LICENSE`, `NOTICE`) are in no commit. The 2D viewer was not modified.

Raw outputs of the studies were kept in the session scratch directory; the instruments are committed in the M12 repository
(`examples/f43_profile.rs`, `examples/f43_hotpaths.rs`, `examples/m9_penalty.rs --mode`, `tests/f43_validation.rs`,
`tests/f43_block_scenarios.rs`, `tests/f43_mechanisms.rs`) and every number below can be regenerated with the commands in section 21.

**Which code the studies measured.** The whole-match profiles, the grids, the dominance matrix, the ladders, the context and the scenario studies ran on the F4.3 configuration as of commits `5bb3bee` to `b2b45a8`. The later M12 commits
are test and structure changes and are bit-identical for every earlier mode (the golden digests are unchanged), with two exceptions inside the F4.3 football itself: a booked player's **aerial** foul is now read with the same leniency as a ground foul
(`4ab8eb2`), and the hold decision's willingness is multiplied in a different order (`b28fe7c`, equal up to float rounding). The pinned F4.3 digest was taken after them, and **the 600-match profile (discipline, shot buckets, penalties) was re-run on the final head `dc6db5e` and is identical to every digit reported in sections 8 and 10**: neither exception changes a number there.

## 1. Executive verdict

**NO-GO: F4.3 does not make the engine final, and F5 should not treat the football as settled.** Three completion gates fail outright (physical expenditure, shot calibration, multi-season validation), four
are only partly met, and three are met. What this run did achieve is real and measured:

* The F4.2 dominance of defensive settings is gone. On the same seeds, `defensive + high press` / `defensive + low press` were ahead of
  balanced in all three columns (mean +0.28 / +0.25 points a match); under F4.3 no setting is ahead of balanced across the eight opponent
  archetypes beyond what chance explains, `very defensive` is clearly worse (-0.26), and the free lunch of the block (F4.2: 5.4 more shots
  conceded for 0.26 fewer goals) is gone (+0.7 shots, +0.04 goals; section 7).
* Fouls now come from something a player does: a beaten defender decides whether to hold a carrier who is running at goal. Yellow cards are
  4.2 a match (Premier League 2023/24: 4.46; Bundesliga 2023/24: 4.15), dismissals 0.17 (about 0.16), straight reds 0.08.
* The in-match penalty conversion was 29 % (a goalkeeper who saved 59 % of the penalties of 79-finishing takers); it is 70 % now, and the
  mechanism behind it (a keeper whose reaction and reach responded steeply to his rating) is corrected, not patched at the result.
* The AI manager's formation choice and its replacement choice are the autonomous manager's own judgement, with tests that fail if the
  score or the utility is ignored.
* Determinism, segmentation equivalence, canonical finalisation and old checkpoints are untouched (the whole M12 suite, including the
  historical golden digests, passes).

**Not achieved** (details and numbers in the sections named):

| Gate | Status | Why |
|---|---|---|
| 1. Defensive-block exploit | **Met** | no defensive setting beats balanced across 8 archetypes (section 4.2); a mild tilt towards pressing sides is left (sections 4.2 and 7) |
| 2. Pressing has benefits and costs | **Partly** | chances and shots conceded fall, a rushed builder fails more passes, and it costs energy, own-third losses and counters; it still does **not** win the ball back more often (section 6) |
| 3. Attacking has context-dependent advantages and risks | **Partly** | the sign depends on the opponent (+0.36 against an ordinary one, -0.10 against a counter side), but against equals attacking is worth +0.24 points a match, more than in F4.2 (section 7) |
| 4. Fouls, cards, penalties follow credible mechanisms | **Partly** | cards, dismissals and penalty conversion are in range; fouls are 15.3 (reference 21 to 27) and penalties awarded 0.46 (reference about 0.3, unsourced) (section 8) |
| 5. Physical expenditure coherent by role | **Not met** | the energy accounting defect is fixed (forward / defender reserve cost 3.7x to 2.3x), but the engine still has midfielders running 17 km and forwards sprinting 2.1 km a match (section 9) |
| 6. Shot selection and scoring calibrated | **Not met** | goals per team and penalty conversion are credible; shot volume (37.5 against 27.6) and mean shot distance (11.9 m against about 16 m) are not (section 10) |
| 7. Match state and manager decisions matter | **Partly** | the manager decisions are tested to depend on the score and chasing now pays (+0.11 points); protecting a lead still has no measurable effect on points, and **numerical strength regressed**: against an organised block fewer defenders concede fewer shots (section 12, D) |
| 8. Multi-season / multi-club validation | **Not met** | seven of twelve club-seasons complete with every invariant held; the engine plays 4.7 to 8.0 goals a match in the user's club's matches (section 13) |
| 9. Save/reload and finalisation | **Met** | a mid-season save and reload reproduces the uninterrupted season exactly; every fixture finalised once; own goals reconcile |
| 10. Determinism and performance | **Met** | 0.776 s a match: +0.5 to +4.6 % over F4.2 (0.742 to 0.772 s on the same machine), +30 % over M12 (section 14) |

## 2. Baseline reproduced (checkpoint 1)

Same seeds, same configuration (`full_match_f42`), release build, M12 head `f4ab176`.

| F4.2 finding | Reproduced? | Evidence |
|---|---|---|
| 1. Defensive + high / low press beat balanced against every opponent | **Yes, digit for digit** | 300 seeds: balanced 1.45 / 1.40 / 1.20; defensive + high press 1.67 / 1.59 / 1.62; defensive + low press 1.69 / 1.50 / 1.59 |
| 2. Attacking mentality suppresses shots conceded against equals | **Yes** | 300 pairs: attacking -0.59 [-1.19, +0.01], very attacking -1.28 [-1.87, -0.69] |
| 3. High pressing does not force more errors | **Yes, and located** | a pressed builder's first pass completes as often as an unpressed one's (92 % against 90 %), because he chooses a safer pass (section 6) |
| 4. Compactness reduces shot quality but raises volume | **Yes** | defensive + high press v balanced: shots conceded +2.1, open chances conceded -1.3, goals against **-0.51** |
| 5. Fouls and cautions low | **Yes** | 10.3 fouls, 2.28 cautions, 0.16 dismissals a match (600 matches) |
| 6. Penalties frequent | **Yes** | 0.49 awarded a match; and only **29 % of them scored** (the keeper saved 59 % in the 400-match run of the first measurement) |
| 7. Forwards spend about five times the defenders' energy | **Yes, and decomposed** | net reserve loss per minute 0.152 / 0.493 / 0.565 for defenders / midfielders / forwards (3.7x); effort spent 10.0k / 16.4k / 17.9k eu (1.8x) |
| 8. Shot distance and on-target rate suspicious | **Yes** | mean distance 10.3 m; 65 % on target; 60 % of shots from 6 to 11 m are saved; 0 goals in 1.7 shots a match from 16 to 22 m |
| 9. +24 % match time | **Yes** | 0.598 s (M12), 0.742 to 0.772 s (F4.2) on the same machine and build |

### 2.1 Root causes, separated as the brief asks

| # | Finding | Class | Mechanism |
|---|---|---|---|
| a | A defensive mentality out-scored balanced | **A + B** | the compact block (`shape_pass`) is a competent defence routine (shifting with the ball, goal-side recovery in the own third, a level back line); F4.2 switched it on below mentality 0.4 and left it off at 0.5, so "defensive" was a better way to defend, not a different one. Its effect is a trade of volume for quality: against it shots conceded rise and goals conceded fall by more |
| b | Attacking mentality suppresses shots | **D (partly)** | territory: the very attacking side wins the ball in the opponent's final 40 m +2.1 times a match; this is football. What is not football is how little it costs (section 7) |
| c | Pressing creates no turnovers | **A (decision) + D** | the pressed carrier adapts: the first pass gains 4.7 m against 6.1 m unpressed, and is as accurate. Pressure changes what he decides, not how well he executes. Forced errors appear against a *high* press (hurried passes: +0.17 failed passes in 8 s against a low press) but the ball is not won back more often |
| d | Fouls half of football | **A** | the engine judged only tackles on the ball carrier: no holding, no tactical fouls, nothing off the ball |
| e | Cautions half of football, almost all dismissals second yellows | **B** | one severity scale, calibrated in F4.2 on a fixture without the missing fouls |
| f | Penalties 29 % scored | **A** | keeper response to rating (penalty goal rate 18 % at rating 60, 75 % at 92 in isolation) and a test fixture whose keepers are strong (E); the isolated experiment with the production configuration gives 72 % |
| g | Defender : forward load 3.7x | **A + A** | (i) recovery credited while walking cancelled its cost exactly (`rest_cost` 1.0: a player walking at 1.4 m/s nets zero); (ii) movement volume: midfielders 16.4 km, forwards 17.1 km already in the Mission 12 mode, 5 to 8 times the football references in high-speed and sprint distance (pre-existing, M9 to M12) |
| h | 10.3 m shots, 65 % on target | **A + B** | the finishing rule of F4.2 (`finish_min_quality`) is **not** the cause of volume (raising it from 0.25 to 0.38 changes nothing: 37.7 and 37.6 shots); shooters are too accurate and keepers too good at close range, and shots from 11 m out almost never score |
| i | +24 % time | **A** | the options of a carrier tripled (three candidates per team-mate): `decide` went from 0.092 s to 0.242 s a match; **not** the reach-time recomputation that was suspected (removing it saved under 1.5 %) |

E (measurement) items found: the in-match penalty conversion of the fixture teams mixes a strong-keeper fixture with the engine; a
full dominance grid of 27 cells takes over two hours at 300 seeds and was replaced by a six-by-three grid that reproduces the same cells;
the harness started from the Mission 12 configuration unless `F42_FLAGS` was set (now announced in the output of every study).
C (uncertainty): 95 % intervals are given throughout; with 88 cells in the dominance matrix, about four significant cells are expected by chance.

## 3. What changed (code)

Everything is behind a switch that is off in `full_match_m12()` and `full_match_f42()`; `full_match_f43()` turns on what measured as an
improvement. `BEHAVIOUR_REVISION` is unchanged: no football behaviour changed without a switch (the whole M12 suite, including the golden
digests of Missions 8 to 12, passes), and a checkpoint stores the full configuration, so an F4.2 match replays as recorded.

| Area | Change | Files |
|---|---|---|
| Defence | every team defends in the compact block; mentality and width move how tight and how deep it sits, continuously (`organised_block`, `block_shape`); the F4.2 step-up of one defender onto an unpressed carrier is off | `defend.rs`, `config.rs` |
| Fouls | a beaten defender decides whether to hold a carrier running at goal (`tactical_fouls`): promise (distance, cover, speed), aggression, and what the foul costs him (penalty, sending-off, second caution) | `runtime/referee_rt.rs`, `referee.rs` |
| Referee | a booked player's borderline second caution is read as careless (`booked_leniency`); one reading function (`read_foul`) for ground and aerial fouls | `referee.rs`, `runtime/aerial_rt.rs` |
| Physical | recovery needs near-standing (`condition.rest_cost` 0.6) | `condition.rs` (unit test), `config.rs` |
| Shot execution | keeper reaction (360 to 260 ms) and dive (0.70 to 1.10 m) ranges flattened; open-play shot error up (0.38 / 0.11 rad); dead-ball factor 0.27 so a penalty keeps its football rate | `config.rs` |
| Manager | `manager::best_formation`, one judgement for the autonomous manager and OFM's assistant | `manager.rs`, `runner/management.rs`, OFM `interactive_match/ai.rs` |
| OFM | plays `full_match_f43`; wear constants re-measured (section 9); season studies | `spatial_runner.rs`, `player_wear.rs`, `interactive_season.rs` |

Measured and **not adopted** (removed from the code): speculative long shots (shots from 11 m and further almost never score, so they only
added 7 to 12 shots a match and no goals), a zonal marking cost (3 % less distance, no balance effect), cover of the carrier's outlets by a
press (shots conceded -3.4 instead of -2.2, but it pushed the pressing sides further ahead), a bound on through-ball candidates (-5 % time, no
effect on the football), and two other block designs: the universal fixed block (`block=1.0`) left every deviation from balanced ahead of it (+0.14 to +0.21 for all but very defensive), and a wider block (`blockdepth=14`) with the F4.2 step-up made attacking + high press dominate (+0.26 mean, three columns).

## 4. Tactical balance before and after

### 4.1 The 6 x 3 grid (points a match, 300 pairs; difference to balanced with its 95 % interval in the full output)

| home setting | F4.2: v balanced | v att+high press | v def+low block | F4.3: v balanced | v att+high press | v def+low block |
|---|---:|---:|---:|---:|---:|---:|
| balanced | 1.45 | 1.40 | 1.20 | 1.29 | 1.22 | 1.53 |
| very defensive | 1.49 | 1.16 | 1.24 | 1.11 | 1.01 | 1.37 |
| defensive + high press | **1.67** | **1.59** | **1.62** | 1.61 | 1.17 | 1.74 |
| defensive + low press | **1.69** | 1.50 | **1.59** | 1.33 | 1.06 | 1.50 |
| attacking + high press | 1.33 | 1.22 | 1.30 | 1.49 | 1.44 | 1.63 |
| very attacking | 1.55 | 1.31 | 1.32 | 1.43 | 1.19 | 1.58 |

F4.2: defensive + high / low press ahead of balanced in all three columns (+0.28 / +0.25 on average). F4.3: defensive + high press +0.33*
and +0.21* against two columns but -0.05 against the pressing opponent (mean +0.16); defensive + low press -0.05; very defensive -0.18.

### 4.2 The dominance matrix (the final instrument): 11 settings x 8 opponent archetypes, 150 paired seeds, F4.3

Opponents keep the home team's overall level (attribute shifts sum to zero, within +-8 points): balanced, possession, fast counter, direct,
high press, low block + counter, technical, physical. Points a match, difference to the balanced setting in the same matchup; `*` = 95 %
interval excludes zero.

| setting | mean difference over the 8 archetypes | cells excluding zero |
|---|---:|---|
| very defensive | **-0.259** | v possession, v fast counter, v high press (all negative) |
| balanced + low press | -0.085 | v balanced (negative) |
| attacking + low press | -0.109 | v fast counter (negative) |
| defensive + low press | -0.084 | none |
| defensive | -0.019 | none |
| balanced + high press | +0.025 | none |
| very attacking | +0.058 | v physical (positive) |
| defensive + high press | +0.075 | none |
| attacking | +0.081 | none |
| **attacking + high press** | **+0.142** | none (7 of 8 cells positive, the eighth -0.04) |

Six of the 88 cells exclude zero (4.4 expected by chance, if the cells were independent). No setting is better than balanced against every
archetype with an interval that excludes zero; the two weakest are the extreme conservative ones. **What is left**: attacking + high press
is ahead of balanced in seven columns (mean +0.14, about 2.6 standard errors of that mean). That is a credible football result between equals
(pressing sides do outperform), but its cost in the engine is energy and condition (section 6), which one match does not charge: section 14
charges it across a season.

### 4.3 Player quality against tactics (200 pairs, F4.3; home is the stronger team)

| setting | 72 v 58: points | win / draw / loss % | 66 v 62 (a league-typical gap): points | win / draw / loss % |
|---|---:|---|---:|---|
| balanced | 2.67 | 85 / 12 / 4 | 1.72 | 50 / 22 / 28 |
| very defensive | 2.50 | 80 / 8 / 11 | 1.74 | 50 / 24 / 26 |
| defensive | 2.66 | 86 / 8 / 6 | 1.76 | 53 / 17 / 30 |
| attacking | 2.79 | 92 / 5 / 4 | 1.92 | 56 / 25 / 20 |
| very attacking | 2.71 | 88 / 6 / 6 | **2.12** | 64 / 20 / 16 |
| defensive + high press | 2.80 | 92 / 6 / 3 | 1.84 | 55 / 20 / 25 |
| defensive + low press | **2.42** | 76 / 14 / 10 | **1.56** | 44 / 26 / 30 |
| attacking + high press | 2.79 | 90 / 8 / 2 | 2.10 | 63 / 20 / 16 |
| attacking + low press | 2.44 | 76 / 14 / 9 | 1.79 | 54 / 19 / 28 |
| balanced + high press | 2.69 | 87 / 8 / 4 | 1.81 | 52 / 26 / 22 |
| balanced + low press | 2.50 | 80 / 10 / 10 | 1.68 | 48 / 24 / 28 |

The strong team keeps its advantage under every setting (76 to 92 % wins at 14 attribute levels; the weaker team still wins 2 to 11 % and draws 5 to 14 %). Quality is worth about 1.2 points
a match here (2.67 against 1.45 between equals); the whole spread of tactics is 0.38 points (0.56 at 66 v 62, where the weaker side is 4 levels down: the spread of tactics there is as large as the quality gap's value, about 0.3 points).
Cells have a standard error of about 0.1 points. The ordering is the same as elsewhere in this report: low pressing and very defensive play cost points, attacking and pressing gain some.


## 5. Defensive organisation (priority P0)

**What was wrong.** `shape_pass` is a good defence: it shifts the untasked players with the ball, drops the lines goal-side of the ball when
the ball is within 40 m of the own goal and keeps a level back line. Only a mentality at or below 0.4 got it. So the grid compared two ways
of defending, not two tactics.

**What was done.** The routine is the same for everybody. The instructions set two distances, continuously (pure function `block_shape`,
three unit tests): the squeeze (mentality: 0.05 per unit of defensiveness; width: 0.08 per unit) and the depth within which the lines drop
behind the ball (6 m per unit of defensiveness). Nothing is a bonus: a balanced team now defends competently, a defensive team gives up
territory and chances (very defensive concedes +2.0 shots and scores -1.2), an attacking team keeps its lines higher.

**Verified.** `every_team_defends_in_the_block` (the three mentalities all show 100+ block labels a match, while a balanced F4.2 team never
did), the F4.2 mechanism suite, the Mission 11 and 12 defence suites and the unchanged golden digests; the dose-response in sections 2 and 4.

**Staged attacks against an organised low block** (`tests/f43_block_scenarios.rs`, 300 staged attacks of 20 s per cell; shots per 100
attacks, F4.2 | F4.3): central penetration 14 to 17 | 18 to 24; wide 7 to 19 | 9 to 12; switch of play 4 to 8 | 6 to 9; ball in behind
4 to 11 | 8 to 12; counter against a high line 3 to 5 | 4 to 5. Goals per 100 attacks are 0 to 2 everywhere. These cells have a standard error
of 2 to 3 shots, so the table shows that no kind of attack gets through an organised block in a staged 20 seconds, not that one kind of attack
is better than another. The F4.3 balanced block concedes more shots to a central penetration (24 against 16): the block's trade of volume for
quality, now for everybody.

## 6. Pressing (priority P0)

**The pipeline, traced** (trigger, movement, angle, lane closure, perception, first touch, decision, execution, duel, error, recovery,
counter): the presser reaches the ball sooner (F4.2: 10.0 s to 5.0 s from a low to a very high press); the carrier's utility counts
the closing speed; a hurried pass loses up to 15 % of its execution quality (less for a composed player); the lane risk of every option
is computed from where the pressers actually are. What is missing is a *reason for the carrier to execute worse*: he chooses a safer, shorter
option, so the pass he makes is as accurate.

**What the controlled tests show** (`f43_mechanisms`, 300 seeds each, same team-mates):
* pressure changes the decision: the first pass of a carrier with a defender at 2.5 m gains 4.7 m towards goal, with one at 14 m 6.1 m
  (`pressure_makes_the_carrier_choose_a_less_progressive_pass`); it completes 92 % against 90 %;
* a high press hurries him: against a press of 90 the builder (defender at 9 m) makes 1.65 passes and fails 0.24 in 8 s, against a press of 10 0.70
  and 0.08 (`a_high_press_hurries_the_builder_into_failed_passes`).

**Whole matches** (200 pairs, press 85 against 50; balanced opponent): shots conceded **-2.2** [-3.0, -1.3], open chances conceded **-3.4**
[-4.1, -2.7], goals against -0.21 [-0.46, +0.05], points +0.07 [-0.20, +0.33]; costs: own-third losses **+1.5**, counter-attack shots conceded
**+0.4**, energy **+8,350 eu (+5.5 %)**, condition at the end **-3.2 points**. Against the technical opponent: shots conceded -3.7, goals against
-0.35, points +0.18. A low press (15 against 50) concedes **+8.4 shots and +9.7 open chances** and saves 14,000 eu (-9 %): low pressing remains
a legitimate choice only for a team that expects fewer chances than it concedes. **Not demonstrated:** possession won in the opponent's final
40 m is +0.6 [-0.1, +1.2], so the benefit comes through the chances the opponent builds, not through more turnovers. That is the remaining
part of gate 2.

## 7. Offensive mentality and defensive exposure (priority P0)

Against an identical balanced opponent (300 pairs), F4.2 | F4.3:

| | very defensive | defensive | attacking | very attacking |
|---|---|---|---|---|
| shots for | -1.57 \| -1.24 | -0.69 \| +0.52 | +1.29 \| +0.95 | +1.72 \| +0.80 |
| shots conceded | +6.82 \| +1.97 | +5.39 \| +0.73 | -0.59 \| -0.93 | -1.28 \| -0.43 |
| open chances conceded | +4.20 \| +1.27 | +3.23 \| +0.53 | -0.36 \| -0.54 | -1.13 \| -0.21 |
| counter-attack shots conceded | +0.87 \| +0.12 | +0.68 \| +0.09 | +0.09 \| -0.21 | -0.06 \| -0.18 |
| goals against | -0.21 \| +0.16 | **-0.26 \| +0.04** | -0.11 \| -0.03 | -0.18 \| -0.07 |
| points | +0.04 \| **-0.18** | +0.06 \| -0.05 | +0.12 \| **+0.24** [+0.03, +0.45] | +0.10 \| +0.14 |
| energy (eu) | +3,247 \| -1,606 | +2,995 \| -2,609 | +103 \| +2,043 | +2,558 \| +6,375 |

The defensive settings no longer buy fewer goals with more shots (F4.2: -0.26 goals for +5.4 shots; F4.3: +0.04 for +0.7). Attacking still suppresses
shots conceded (territory) and is now worth more points against equals (+0.24).

**The opponent context** (very attacking + high press against balanced, 300 pairs), F4.2 | F4.3:

| | ordinary opponent | counter-attacking opponent (stronger in transition, same level) |
|---|---|---|
| shots conceded | +0.04 \| **-2.05** [-2.73, -1.38] | **+2.78 \| +1.26** [+0.44, +2.08] |
| counter-attack shots conceded | +0.10 \| -0.03 | **+0.41 \| -0.06** |
| goals against | -0.34 \| -0.24 | +0.29 \| +0.24 [-0.02, +0.50] |
| points | +0.06 \| **+0.36** [+0.14, +0.58] | -0.11 \| -0.10 [-0.27, +0.08] |

The sign of the result depends on the opponent (the difference between the two columns in F4.3 is +0.46 points), so attacking is not
universally safer; but against an ordinary opponent it is more profitable than in F4.2, and the exposure against the counter side is
visible in the shots (+1.3) and goals (+0.24) conceded rather than in the counter-attack tally. **Unresolved**: the engine under-prices
attacking commitment between equals. A full fix needs the attacking side's productivity (chances created per possession) to saturate, which
this run did not touch.

**A full-back caught forward** (`a_full_back_caught_high`, a study, 300 seeds): not demonstrated. With the left-back forward, the counter
reaches the box as often (0.99 against 0.94) and ends in a shot **less** often (0.15 against 0.29). In this staging the left-back is behind the
ball and the other defenders are back; I did not find a staging that isolates the exposure, and I did not bend an assertion to pass.

## 8. Discipline (priority P1)

References, with competition, season, sample and definition (not targets): **Premier League 2023/24**, 380 matches, 1,694 yellow cards
= **4.46 a match** (tournamentfootball.co.uk statistics page); **Bundesliga 2023/24**, 306 matches, 1,269 yellow cards = **4.15** (DFB data
center); red cards about 0.16 a match and 62 to 64 sendings-off a season in the decade before (The Analyst, via the F4.2 report); fouls a
match across the big leagues **20.9 (Premier League) to about 27 (La Liga)** (CIES Football Observatory, period not stated); penalties about
0.3 a match (unsourced estimate); penalty conversion about 77 % (widely published; no single source checked in this run). The sources differ
in period, in whether a second yellow counts as a card and in the definition of a foul; two leagues are not a population.

**Mechanism.** `tactical_foul`: a defender who is level with or behind a carrier running at goal at 3 m/s or more and within 2.2 m
decides, each 50 ms, with a chance that rises with the *promise* of the attack (`attack_promise`: distance to goal, 40 % off per defender
goal-side, speed), with his aggression, and is cut when the foul would be a penalty (x0.35 x0.05), a sending-off (x0.15 for an obvious chance) or
a second caution (x0.4). The referee reads it with the ordinary scale plus an increment for denying an obvious chance (0.5), and a booked player's
borderline second caution is read as careless (0.35). Nothing is scheduled; every foul has an offender, a victim and a position.

**Tests.** `a_beaten_defender_fouls_a_promising_attack_only_when_the_decision_exists` (0 against 57 fouls in 120 staged attacks),
`a_run_far_from_goal_is_not_worth_a_foul`, `a_covered_attack_is_held_less_than_an_open_one` (57 against 1),
`only_a_hold_that_denies_an_obvious_chance_carries_the_denial_term` (identical holds, 2 cautions + 3 dismissals against none when the weight is
removed; identical when a defender covers), and three pure unit tests (`attack_promise`, `read_foul`, `block_shape`).

**Result** (600 whole matches, both teams, per match; 95 % interval):

| | M12 | F4.2 | **F4.3** | reference |
|---|---:|---:|---:|---|
| fouls | 10.77 | 10.33 | **15.32** [15.0, 15.6] | 20.9 to 27 |
| careless / reckless / excessive force | | 8.0 / 2.3 / 0.04 | 11.0 / 4.2 / 0.08 | |
| yellow cards (first) | 0.86 | 2.28 | **4.23** [4.07, 4.38] | 4.46 (PL), 4.15 (BL) |
| second yellows | 0.02 | 0.12 | 0.09 | |
| straight reds | 0.02 | 0.04 | **0.08** | |
| dismissals | 0.04 | 0.16 | **0.17** [0.14, 0.21] | about 0.16 |
| penalties awarded | 0.33 | 0.49 | 0.46 [0.40, 0.51] | about 0.3 (unsourced) |
| penalties scored of those taken | 38 % | 29 % | **70 %** | about 77 % |

Straight reds are now about half of the dismissals (F4.2: a quarter). Referee strictness still only moves the thresholds: the draws of a
referee's match-long strictness and judgement noise are unchanged. **Remaining**: fouls are about 60 to 70 % of football's; the engine still has
no off-the-ball fouls, no fouls in aerial duels beyond the F4.2 ones and no late challenge on a player who has just released the ball, and the
foul count of football is out of reach for tackles and holds alone. Penalties are still above the reference: they are fouls in the box
(58 % of the shots are struck from 6 to 11 m). OFM's suspension integration is unchanged (3 matches for a straight red, 1 for a second yellow,
`Player::can_play()`), and the player statistics reconcile through the same report path (the OFM suites pass, section 15).

## 9. Physical load and fatigue (priority P1)

**Decomposition.** Per player who played at least 85 minutes (600 matches, identical teams, F4.2 | F4.3):

| line | distance km | high-speed m (>=5.5 m/s) | sprint m (>=7 m/s) | effort eu | reserve lost, points per minute |
|---|---:|---:|---:|---:|---:|
| defenders | 9.54 \| 10.76 | 1,814 \| 1,583 | 653 \| 511 | 9,989 \| 10,680 | 0.152 \| 0.255 |
| midfielders | 16.57 \| 17.13 | 5,190 \| 5,211 | 1,739 \| 1,722 | 16,423 \| 16,803 | 0.493 \| 0.554 |
| forwards | 17.72 \| 17.45 | 5,897 \| 5,317 | 2,480 \| 2,104 | 17,924 \| 17,413 | 0.565 \| 0.585 |

References (published tracking studies of top-flight outfield players, as I know them; **not re-sourced in this run**): total distance about
10 to 12 km for every outfield role, high-speed running (above 19.8 km/h) about 0.6 to 1.0 km, sprinting (above 25.2 km/h) about 0.2 to 0.3 km,
forwards within 10 % of defenders in total distance and about 1.5x in high-speed running.

* **The energy accounting defect (fixed).** `rest = clamp(1 - cost / rest_cost)` credited a walking player (1.4 m/s, cost 0.53) with 47 % rest, and the
  long-term recovery at that rest equals the cost of walking: a defender who walked all match lost nothing. With `rest_cost` 0.6 walking is not rest
  (`walking_is_not_rest_when_rest_means_standing`: a 90-minute walk costs 0.00 of the reserve with 1.0, more than 0.07 with 0.6). The forward : defender
  cost falls from 3.7x to 2.3x (0.152 / 0.565 to 0.255 / 0.585 points a minute).
* **The movement volume (not fixed).** Midfielders run 17 km and forwards 17.5 km a match, high-speed running is 5 to 8 times the reference and sprinting
  7 to 10 times; this is the Mission 9 to 12 movement calibration (the Mission 12 mode gives 16.4 and 17.1 km), and `CALIBRATION_M10_REPORT.md` already
  recorded "14.9 km, not 10 to 12". The breakdown by ordered activity shows where it goes: a forward spends 29 minutes in formation movement
  (4.4 km), 12 minutes in runs (3.6 km), 8 minutes pressing (2.4 km) and 9 minutes marking (2.4 km); a midfielder carries the ball 4.3 minutes (1.3 km, 0.95 km at high speed).
  I tried the levers that exist and none is a correction: the Mission 12 need-based urgency (scope 3: shots double), the Mission 10 press leash
  (distance -30 % but passes -45 % and shots +50 %), a longer rest between runs, a larger formation dead band (no change) and a zonal marking cost
  (-3 %). Reducing the movement volume needs a new movement model, which is outside this run.
* **Hybrid career model.** Kept: OFM compares a match with the neutral workload of the player's own line, one authoritative post-match update.
  The neutral workloads change with the engine: re-measured on 300 / 221 / 94 starters of 120 FM24 matches (a read-only copy of the world): **0.00147 / 0.00391 / 0.00486** (F4.2: 0.00068 / 0.00327 / 0.00422; forward : defender 3.3x from 6.2x). The drift study then reads a mean load ratio of **0.98 / 1.03 / 1.00** for defenders / midfielders / forwards: in-sample (the same 120 matches), no held-out set was run. Before the update it read 1.30 for defenders and the study failed, as designed.

## 10. Shots, chances and scoring (priority P1)

Open-play shots a match (both teams, 600 matches), F4.2 | F4.3:

| distance | shots | share | on target | goal % of shots | saved / blocked / off target % |
|---|---:|---:|---:|---:|---|
| 0 to 6 m | 0.55 \| 0.33 | 2 \| 1 % | 80 \| 78 % | 31.2 \| 23.4 | 48 / 8 / 12 \| 54 / 7 / 15 |
| 6 to 11 m | 20.6 \| 21.7 | 67 \| 58 % | 73 \| 61 % | 12.3 \| 9.4 | 60 / 6 / 21 \| 50 / 8 / 32 |
| 11 to 16 m | 6.7 \| 10.4 | 22 \| 28 % | 54 \| 28 % | 4.3 \| 4.7 | 49 / 13 / 33 \| 22 / 19 / 54 |
| 16 to 22 m | 1.7 \| 3.5 | 6 \| 9 % | 35 \| 17 % | 0.3 \| 0.6 | 34 / 16 / 49 \| 16 / 23 / 61 |
| 22 to 30 m | 1.0 \| 1.5 | 3 \| 4 % | 24 \| 12 % | 0.2 \| 0.0 | 24 / 14 / 63 \| 12 / 18 / 70 |
| **total** | **30.5 \| 37.5** | | **65 \| 46 %** | goals 2.99 \| 2.63 | |

* **Is the F4.2 finishing rule to blame?** No. `finish_min_quality` 0.25 replaces a carry by a shot when the carrier is unpressed and the chance is clear; with
  0.32 and 0.38 the volume is 37.7 and 37.6 (0.25: 37.5). Volume comes from the block (section 5): everybody in a compact block concedes shots
  from 11 to 22 m (10.4 and 3.5 against 6.7 and 1.7). The rule stays.
* **Execution (changed).** A goalkeeper whose penalty-save rate went from 18 % to 75 % between ratings of 60 and 92 is not a keeper, it is a threshold;
  and 60 % of the shots struck from 6 to 11 m were saved. The reaction range (320 to 130 ms) became 360 to 260 ms, the dive range (0.70 to 1.55 m) 0.70
  to 1.10 m, the open-play aim error 0.26 / 0.045 became 0.38 / 0.11 rad, with the dead-ball factor 0.4 re-set to 0.27. Penalties converted: **70 %** in whole
  matches (isolated: 64 % of 100 at the F4.3 configuration, 72 % at F4.2). Shots on target fall from 65 % to 46 % of shots (the Premier League's published
  2023/24 figures are 50 % for shots on target on one source and about a third on others: I could not pin a definition, so I do not claim a match);
  goals a match are 2.63 + 0.32 penalties.
* **Goals** per team and match: 1.5 (Premier League 2023/24: 1.64; F4.2 1.77). **Career impact**: standings, player statistics, suspensions, awards
  and finances read the same report; the OFM suites (section 15) and the seasons (section 13) pass. The economy was not retuned for the 17 % fall in goals.
* **Not met:** 37.5 shots a match against the Premier League's 27.6 (both teams, 2023/24, one source); mean shot distance 11.9 m against about
  16 m; 58 % of shots from 6 to 11 m; goals from 16 m and beyond are 0.02 a match. The block raised the volume, and I could not lower it without
  removing the block's effect, speculative shots only make it worse, and a distance-dependent keeper would move the economy again. Left as the main open item
  of gate 6.

## 11. AI manager and tactical intelligence

* **One judgement for the shape.** The autonomous manager (`sim/runner/management.rs`) and OFM's assistant now both call `manager::best_formation`
  (`formation_utility` over M12's six presets, a price on a change, an extra price for going back to the shape just left). OFM used to hard-code 4-5-1
  for ten men and 4-3-3 for a man up. Behaviour that follows, pinned by literal tests: ten men get a back five when level (5-3-2) and keep 4-4-2 when
  chasing late; a side that set up with a back five and is two goals down at 75' goes 4-3-3; two goals up at the break it protects with 5-3-2; a level game
  with the numbers level keeps its shape; a deficit alone is chased with the style, not the formation; going back to the shape just left costs.
* **Replacement.** F4.2 shared `substitution_utility_v2`. The parity test that the F4.2 reviewer found non-discriminating is kept as a wiring test (its doc says so)
  and `the_score_decides_which_bench_player_comes_on` is the discriminating one: two bench midfielders of equal quality in the slot, a finisher and a stopper;
  the finisher comes on when chasing, the stopper when protecting (margins 0.05 to 0.075 in utility); mutation checks by the test reviewer: ignoring the score and
  flipping its sign both fail it.
* **Still two implementations**: opponent-momentum sensing (`opp_pressure`) and the short-term fatigue and injury handicap terms exist only in the autonomous manager;
  the tie rule and the "shape just left" are exact in the autonomous manager and inferred from the club's identity formation in OFM.
* **Oscillation.** The autonomous manager's rate limit and the assistant's cooldown are unchanged; no new oscillation was seen in the seasons.

## 12. Match scenarios (A to K)

Same seeds and same starting state for every alternative (300 seeds where the state arose naturally); F4.2 in brackets; 95 % intervals.

| | Scenario | Alternatives from the same state | Result (F4.3) |
|---|---|---|---|
| A | Protect a 1-0 lead at 70' | keep attacking / protect (block + less press) / block only / less press only | wins 75 / 73 / 79 / 74 %; points against "keep attacking": protect **-0.07** [-0.23, +0.09], block only +0.07 [-0.08, +0.22], less press only -0.01 [-0.15, +0.13]; shots conceded +0.05 [-0.27, +0.38] for the full protection; it saves **1,400 eu**. **No measurable benefit and no harm** (F4.2: the same) |
| B | Chase 0-1 at 70' | stay balanced / go for it | points **+0.11 [+0.001, +0.22]** (F4.2: -0.01 [-0.10, +0.08]); goals for +0.05 [-0.05, +0.15]; costs 690 eu. The first measurable benefit of chasing |
| C | A red card at 35' | eleven men / ten men keeping the structure / ten men reorganising | shots conceded after +0.56 [+0.06, +1.06] (F4.2: +1.10), goals against +0.11 [-0.04, +0.27], points +0.007 [-0.16, +0.18]; reorganising (4-5-1): shots conceded -0.05, goals against -0.003, points -0.03: **no benefit** (F4.2: goals against -0.15 [-0.30, -0.01] for +2.2 shots). The cost of a dismissal is visible in shots, not resolved in points |
| D | Nine men | the same staged attack (20 s) against 11 / 10 / 9 defenders | shots per attack **0.31 / 0.21 / 0.10** [0.26-0.36 / 0.16-0.26 / 0.07-0.13] under F4.3 against 0.083 / 0.100 / 0.120 under F4.2. **Reversed**: with the organised block the side with fewer men concedes fewer shots. Forward passes rise as before (0.13 to 0.79), box entries do not fall. Without the block (`noorganised`) the three are flat (0.110 / 0.117 / 0.097); no single block parameter (guard depth, squeeze, ball shift, spacing) restores the F4.2 ordering. **A regression of the F4.2 gate E that I did not resolve** |
| E | Three substitutions after sustained high pressing | keep the tired / fresh legs at 70' | condition at the end +8.2 points (F4.2: +7.2); points **+0.013 [-0.09, +0.12]**: freshness alone does not move chances or points |
| F | An attacking full-back caught out of position | in place / caught forward | **not demonstrated** (section 7): shots 0.15 caught against 0.29 in place, ball in the box 0.99 against 0.94 |
| G | A technically strong team against a high press | press 85 against 50, opponent technical | the press **still wins**: shots conceded -3.7 [-4.5, -2.9], goals against -0.35 [-0.60, -0.10], points +0.18 [-0.07, +0.43]. No sign that technique lets a side escape it; a limitation of gate 2 |
| H | A low block defending repeated attacks | staged (section 5) and the "low block + counter" archetype | no staged attack beats an organised block in 20 s (shots per 100 attacks 4 to 24, goals 0 to 2); against the archetype the defensive settings are within noise of balanced (mean -0.02 to +0.08) |
| I | Strong team against weak team | 72 v 58 and 66 v 62, every setting (section 4.3) | the strong team wins 76 to 92 % at 72 v 58 (weaker side wins 2 to 11 %), and 44 to 64 % at 66 v 62 |
| J | Two matches within four days | the seasons' fixtures within four days of the last (7 to 14 a season) | the players who play arrive with 89 to 97 against 89 to 99 after a week's rest: congestion costs 0 to 2 points of condition, no spiral (section 13) |
| K | A full season with suspensions, injuries and fatigue | section 13 | |



## 13. Season and career validation

The season harness (`src-tauri/src/application/interactive_season.rs`) plays every fixture of the user's club (the strongest English club of a generated world) as an interactive M12 match
through the application layer, finalises it through the one canonical path, and advances the days between matches as the game does. Four ignored studies ran on the final code (release, FM24 world
as a read-only copy): the single season with a save and reload after the tenth match (seed 7), three generated worlds (seeds 11, 23, 37), three plans on seeds 11 and 23 (low block, balanced, high press) and two clubs of
the FM24 world (the best rated, 67.7, and a middle one, 57.3). **Twelve club-seasons were played; seven ran the whole 38 matches, five ended with the board dismissing the manager (after 3, 13, 32, 33 and 35 matches).**

What the seven complete seasons showed (all assertions passed): every league fixture finalised once (the club's table row equals its completed league fixtures); every table row equals its results
(`standings_match_results` finds no fault); nobody injured or suspended played (0 violations over about 270 matches); no condition spiral (in the first third of a season the players who played arrived with 92 to 97, and
the drift from the first to the last third was at most 6.5 points: 92.3 to 85.8 for the balanced plan on seed 23, 91.7 to 86.8 for the high press on seed 11; the lowest arrival was 50; squad average after a match never below 63);
reds a season 1 to 13 (all kept out, see section 8); the reloaded career plays the rest exactly like the uninterrupted one (equal fingerprints: results, players, staff, inbox).

* **Scenario J, fixtures within four days** (7 to 14 per season): the players who play arrive with 89.2 to 97.2 against 89.3 to 98.5 after a rest of six days or more: congestion costs 0 to 2 points of condition, not a spiral.
* **What a press costs across a season** (seed 11, same club, same world): high press squad condition before matches 89.4 and after 72.7 (a match costs 16.7 points); low block 96.7 and 84.0 (12.7). Points 41 against 57. The comparison could not be completed on a third plan (the balanced plan's season was cut short by a dismissal on seed 11, the high press's on seed 23), so the "a press costs more condition than a low block" assertion ran on no seed; the numbers above are the evidence, from one seed.
* **Injuries**: 0.4 to 1.1 players of the squad injured after a match on average.

**Defects and findings of the seasons (gate 8 fails):**

1. **Goals in the user's matches are 4.7 to 8.0 a match** (seed 7: 5.4; seed 11: 7.7 low block, 4.7 high press; seed 23: 8.0 low block, 5.1 balanced, 6.0; seed 37: 5.3) while the same worlds' other matches, mostly produced by
   the statistical model of the followed-leagues scope, average 2.85 to 2.92 and real football 2.7 to 3.3. **This is not new**: the same season on the F4.2 football gave 5.1 goals a match (seed 7), and it is not seen on the engine's own fixtures
   (2.5 goals at equal level), where goals grow with the quality gap (open-play goals 2.45 at 65 v 65, 3.15 at 72 v 58, 4.38 at 75 v 50). The user's club is the strongest of its world, and the engine amplifies a gap of 15 to 25 attribute
   levels into 5 to 8 goals a match. F4.3's recalibration of execution did not change that; I did not find the cause in the attribute-to-outcome functions and did not have the run left to.
2. **The manager is dismissed in 5 of 12 seasons**, including by the best FM24 club after 3 matches (0 league matches played) and by a middle FM24 club after 13: a board that dismisses the harness manager (born 1980, no standing) that early is a property of the harness and of the board model as much as of the engine, and I did not investigate it. Where
   the club is the strongest in a generated world, 22 points from 26 league matches is not credible for a strong side, and does follow from the 4.7 to 8.0 goals above (a side that concedes 3 a match loses).
3. The season test's threshold of 30 league matches in 38 was wrong (38 includes cup ties): it is now 25.
4. An existing OFM test, `a_managed_match_is_finalised_once_and_survives_a_reload`, failed once the football changed because it asserted that player goals equal the score; an own goal is credited to nobody. It now counts own goals (the stored record names the scorer). Not a defect of the engine; a test written for a seed without one.


## 14. Performance (Apple Silicon, release, quiet machine, `examples/f4_perf.rs`, 10 matches)

| | M12 | F4.2 head `f4ab176` | F4.3 |
|---|---:|---:|---:|
| whole match | 0.598 s | 0.772 s (+29 %) | **0.776 s** (+30 % over M12, +0.5 % over F4.2 as built here) |
| one-minute slices | 0.604 s | 0.754 s | 0.784 s |
| view() + JSON | 11.6 us | 12.0 us | 12.6 us |
| tactical command | 156.7 us | 186.1 us | 157.1 us |
| checkpoint at 30' | 100,390 B | 100,387 B | 100,388 B |
| restore | 228 ms | 288 ms | 290 ms |

Where the time goes (`examples/f43_hotpaths.rs`, seconds a match): M12 0.645 (decide 0.092, team planners 0.139, begin tick 0.324), F4.2 0.789 (decide **0.242**),
F4.3 0.817 (decide 0.260). **The suspected hot path was wrong**: the reach-time work for options that cannot use it (removed, bit-identical) saved under 1.5 %;
bounding the through-ball candidates to the 2 furthest-ahead team-mates saved 5 % and changed nothing in the football, and was not kept. The sample of the running engine
shows `hypotf` (every `dist`), `generate_options` and `reach_time_s`; replacing the distance function would change every golden digest. The cost of F4.2's
realism stays: the options per decision tripled. A 38-match season is about 30 s of engine time.

## 15. Regression suites

| Suite | Result |
|---|---|
| M12 `cargo test --release --no-fail-fast` at `b28fe7c` (the last code commit; `dc6db5e` changes one test file, whose suite was re-run) | **621 passed, 0 failed, 29 ignored (studies)**; every historical golden digest (Missions 8 to 12, F4 session equivalence, F4.2) unchanged |
| M12 `f43_mechanisms` at `dc6db5e` | 10 passed |
| M12 `clippy --release --all-targets -D warnings`, `cargo fmt` | clean |
| OFM `cargo test --locked --workspace` (against the local engine at the final head; the pin was verified separately) | **4,457 passed, 0 failed, 61 ignored** (F4.2: 4,448 / 0 / 58) |
| OFM `--features mcp --lib` | 633 passed, 0 failed |
| OFM clippy `--locked --workspace --all-targets -D warnings`, default and `--features mcp`; `cargo fmt --check` | clean |
| `npm test` | 331 files, 2,913 passed, 4 skipped, 0 failed (includes the `src/i18n` coverage gates) |
| `npm run build`, `lint` | clean |
| `npm run format:check` | **was failing on `SuspensionBadge.test.tsx`, a F4.2 file** (the F4.2 report said clean); formatted, now clean |
| Ignored studies run | dominance matrix (88 cells, 150 pairs), quick grids (6 x 3, 300), ladder and opponent context (300 pairs, F4.2 and F4.3), pressing levels, quality, scenarios 1 to 4 and E (F4.2 and F4.3), staged block scenarios, FM24 workload (twice), season studies (twice, the first run on a stale build was discarded) |
| **Not run** | `mcp-e2e`, the mutation-tests workflow, `cargo test --bin`, the desktop application interactively (section 17), a held-out set for the wear constants |

Two pitfalls found on the way, recorded for whoever repeats this: (1) a source file restored with `mv` from a backup keeps its old modification time and Cargo then believes the build is current: an experiment with the football
switched to F4.2 and back left the next test binary on F4.2 and produced a season run that looked plausible (5.09 goals a match) and was wrong; the OFM test `every_ofm_match_is_played_with_the_f43_football` would have caught it and the run was discarded and repeated;
(2) in zsh an unquoted `$flags` is not word-split, so a shell loop over study flags silently ran with one malformed flag (`${=flags}`).


## 16. Reviewers (the five read-only agents of CLAUDE.md)

The five agents of CLAUDE.md ran on the diff, their findings were acted on, and the three that matter for the code ran again (architecture, dedup, tests); a narrow third architecture
pass covers the last commits. Reviewers read both repositories (the engine is where most of the change is).

| Reviewer | Round 1 findings | Disposition |
|---|---|---|
| architecture | **blocking:** OFM pinned an M12 revision without `full_match_f43`; a dangling doc link; a dead `_need` parameter; the half-time formation behaviour change had no test; `shape_pass` 131 lines, `apply_f42_flag` 150, `check_season` 108 | M12 pushed and OFM re-pinned (verified with `--locked`, local override removed); report written; parameter removed; literal half-time / return-charge / deficit tests; `shape_pass` 97 lines (`block_shape`, `nearest_untasked`, `level_back_line`), `apply_f42_flag` 84, `check_season` 52; unchanged: `generate_options` (267 lines, pre-existing; F4.3 netted +12) |
| i18n | none: no player-visible string, no Rust path emits prose; holds are ordinary `Foul` events with existing keys | no change. Advisory kept: the foul commentary templates use `{{victim}}` and the replay path may leave it empty (pre-existing; holds make it slightly more visible) |
| accessibility | nothing in remit: no `src/` change except formatting of one F4.2 test | the formatting failure of `SuspensionBadge.test.tsx` (an F4.2 file that failed `format:check`) was fixed |
| dedup | aerial foul path copies the judgement; formation argmax three times; `f43_penalty.rs` copy of `m9_penalty.rs`; determinism test copy; seeds helper five times; club strength | `judge_for` reads aerial fouls too; `manager::best_formation`; merged into `m9_penalty --mode`; shared helpers in `f42_common`; stored `ovr`; `engage_weight` and `cautions_on_record` shared by the ground, hold and aerial readings |
| tests | 12 findings: the OFM seam untested; the vacuous man-up test; unguarded half-time and return charge; `booked_leniency`, hold promise and block shape unguarded by behaviour; the chaotic single-seed spread claim; the covered-attack doc; studies silently measuring M12; the season study asserting nothing | each fixed as listed in section 11 and in the M12 commits: pure `read_foul`, `attack_promise`, `block_shape` with unit tests, a pinned F4.3 digest, literal AI tests, an OFM test that the football every match uses is F4.3, studies announce their football, the season study asserts that a press costs condition |

| Reviewer | Round 2 | Disposition |
|---|---|---|
| architecture | non-preset formations (3-4-3, 4-1-4-1) could no longer be reshaped; the season study turned a board dismissal into a NaN assertion failure; the report said "literal" for tests that compare with an oracle; "the shape just left" is inferred from the club's identity; `check_season` length | `nearest_preset` (with a test); the dismissal is an explicit outcome; wording corrected; the inference is limitation 8; `check_condition` extracted |
| dedup | aerial sanction chain still its own copy (it ignores pending cautions and downgrades a keeper's red to a yellow even where `goalkeeper_sendoff` is on); "booked" read three ways; the willingness weights copied; magic 26 / 4 | the aerial divergence **predates this run and changing it alters Mission 9 to F4.2 outcomes: left, and recorded for epic #589**; `cautions_on_record`, `engage_weight`, `hold_obvious_goal_m` / `hold_obvious_mps` |
| tests | mutation survivors (listed below) | see below |

**Mutation survivors reported by the test reviewer and not closed** (all in the seams between a tested pure function and its use):
the engine feeding the offender's real booked state into `read_foul`; `block_parameters` reading the right side's mentality and width (only `block_shape` is unit-tested and the digest runs at 50 / 50);
`match_football_config` called from the instant path (`spatial_runner.rs:132`) and the interactive path (`interactive_match/mod.rs:312`): reverting either call site to F4.2 keeps every test green, the function itself is guarded;
the clock in `formation_for` and the tie rule of `best_formation`; small calibration constants (`excessive_at`, keeper reaction) are guarded only by the pinned digest and the ignored studies; the plan-comparison season study is an ignored study.
Mutations that **die**: `tactical_foul` off, the hold promise's distance, cover and speed terms, the denial term, the in-area deterrent, `organised_block`, `block_shape`'s squeeze sign and bound, `rest_cost` back to 1.0, the OFM configuration revert (function level), the AI formation and substitution mutants (score ignored, score sign flipped, men ignored, return charge, gain threshold).


## 17. Desktop playability

**The desktop application was launched; it was not played.** `npm run tauri dev` (the supported development workflow: Vite plus the Tauri shell, built against the local engine) compiled and started:
the process `target/debug/openfootmanager` ran, the webview loaded the frontend from `http://localhost:1420` and logged its startup line (`AP Football Manager v0.3.0-dev`, commit `aec61a7`), and the backend answered
`get_settings` and `get_active_game` (no game loaded). I then stopped it. **I had no way to drive or see the window**: no desktop-control tools were available to this session and a screen capture was refused
(`could not create image from display`). So none of the following was done, and nothing in this report is a manual desktop test: selecting a team, setting tactics, starting a match, changing mentality, a
substitution, responding to a dismissal, saving during the match, quitting and reloading, finishing it, and inspecting the standings, statistics and suspensions. The application-level equivalents that did run
are the interactive matches of the season studies (section 13, a save and reload mid-season) and `a_managed_match_is_finalised_once_and_survives_a_reload`, which are tests through the same commands, not a person at a screen.
The launch may have created the application's own settings directory; no career and no save was opened or changed. The playthrough of F4.2's section 16 still applies and should be done by a person.


## 18. Remaining limitations (all of them)

1. **Gate 5, physical realism.** Midfielders run 17 km and forwards 17.5 km a match; high-speed running is 5 to 8 times, and sprinting 7 to 10 times, what published tracking
   studies give. Only the energy accounting was fixed. The Mission 10 levers move volume at the price of the football (section 9); a new movement model is needed.
2. **Gate 6, shots.** 37.5 open-play shots against 27.6 and a mean shot distance of 11.9 m against about 16 m; 58 % of shots from 6 to 11 m; almost no goals from 16 m out.
   The compact block raises shot volume for everybody; the finishing rule is not the cause.
3. **Gate 2, pressing.** The ball is not won back more often by a high press (+0.6 [-0.1, +1.2] in the opponent's final 40 m); its benefit is the chances the opponent
   builds. A technically strong opponent does not escape it.
4. **Gate 3, attacking.** Against equals it is worth +0.24 points a match and suppresses shots conceded; the context dependence exists (+0.36 against an ordinary opponent,
   -0.10 against a counter side) but is smaller than the edge. The attacking side's productivity is not saturating.
5. **A mild tilt towards attacking + high press** in the dominance matrix (+0.14, seven of eight archetypes; no single cell significant).
6. **Discipline.** Fouls 15.3 (reference 21 to 27); penalties 0.46 (about 0.3, unsourced); no off-the-ball fouls, no late challenges after release; on FM24 club pairs (40 matches, wide intervals) the
   F4.3 referee gives 20.3 fouls, **6.4 cautions and 0.33 dismissals** a match (references 4.5 and 0.16): the thresholds were fitted on the engine's fixture teams and only *checked* on FM24
   players, whose aggression and tackling vary more. Not re-fitted: 40 matches cannot support it.
7. **Protecting a lead** has no measurable effect on points (-0.07 [-0.23, +0.09]); the cost of ten men is visible in shots, not resolved in points; a full-back caught forward is not demonstrated; **numerical strength regressed** (section 12, D): in the staged attack 11 / 10 / 9 defenders concede 0.31 / 0.21 / 0.10 shots, the reverse of F4.2.
8. **Manager.** Opponent-momentum sensing and the short-term fatigue and injury handicap still exist only in the autonomous manager; the assistant infers "the shape just left" from the club's identity formation;
   the autonomous manager changes formation rarely (a deficit alone never does at the break).
9. **Wear constants are in-sample** (re-measured on the same 120 FM24 matches that check them).
10. **Seasons**: see section 13 for the worlds, clubs and seeds actually played.
11. **Performance**: 0.776 s a match, +30 % over M12; the options per decision tripled in F4.2 and are not bounded.
12. **The economy** (goals 17 % lower than F4.2) and the OFM career's calibration of wage, price and reputation were not re-tuned.
13. **References**: the real-football figures are from the sources named in section 8 and from my recollection of published tracking studies (section 9, not re-sourced); the penalty-conversion
    and shots-on-target references are not tied to a single checked source.
14. **GUI**: see section 17.
15. **CI**: see section 20.


## 19. Git and push status

| Repository | Branch | Base | Head |
|---|---|---|---|
| OFM | `claude/run-f4-3-final-balance` | `50291c7a` (F4.2) | the last commit of the branch (this report is part of it): `git log -1` |
| M12 | `f4.3/final-balance` | `f4ab176daea68f280b6791be4242ea4742bd6cea` (F4.2) | `dc6db5ea94b9862f32f4364a978df2c070d588f5` |

M12, oldest first: `5bb3bee` tactical fouls, organised block, booked-player leniency, flatter keeper, walking is not rest, the F4.3 configuration and instruments; `e0e0baf` pressure tests; `b2b45a8` the two mechanisms measured and
not adopted removed, staged scenarios; `5faebf9` perf probes; `c7c3ab6` the full-back study; `4ab8eb2` reviewer findings (one `best_formation`, one reading for aerial and ground fouls, shorter functions, shared helpers);
`f1c9f4d` pure unit-tested leniency, hold promise and block shape, a pinned F4.3 digest; `b28fe7c` one engage weight, one caution count, named thresholds; `dc6db5e` the denial test.

OFM, oldest first: `b9cfa7fc` F4.3 football in every match, the manager's shape from the autonomous utility, season studies; `8bf52206` `best_formation`, half-time and deficit tests; `904b3bf8` neutral workloads re-measured, pin, literal tests, report;
`32976b10` non-preset formations, dismissal as an outcome in the season study; `bf31e893` own goals reconcile, strict `nearest_preset`; `aec61a74` clippy; then the report.

## Push status
M12 `f4.3/final-balance` is **pushed** to `origin` (private) at `dc6db5e`. OFM `claude/run-f4-3-final-balance` is pushed with this report. Nothing was merged into `main`, `develop` or an integration branch. The uncommitted licence files in M12 (`Cargo.toml`, `LICENSE`, `NOTICE`)
and in the viewer were left as they were.


## 20. Secrets and the pinned engine

The F4.2 CI workflows fetch the private match engine with `APFM_ENGINE_READ_TOKEN` through the composite action `.github/actions/private-engine-access` (the token is written to the runner's git
configuration only, and `deny.toml` allows exactly the engine repository). In this run `gh secret list --repo augustinpatte/ap-football-manager` (names only, nothing printed or stored)
**lists no repository secrets**, so the secret still does not exist. **Required user action:** create a fine-grained, read-only token for `augustinpatte/apfm-match-lab` and add it as the
repository secret `APFM_ENGINE_READ_TOKEN`; until then CI cannot build this branch (nor F4.2's). No credential was created, printed or committed.

The pin: `src-tauri/crates/ofm_core/Cargo.toml` and `Cargo.lock` name M12 revision `f1c9f4d72771d4598078578ba5f1c9a6334428c4`, which is pushed to `origin/f4.3/final-balance`. It was verified with
`cargo check --locked -p ofm_core -p db` in a separate target directory **with the local override removed**, fetching the pinned revision from the private repository (the machine's `gh` credentials), which succeeded.
The git-ignored `src-tauri/.cargo/config.toml` (a `paths` override for local development) was restored afterwards and is in no commit.


## 21. Reproduce

```bash
# M12 (~/apfm-match-lab, branch f4.3/final-balance). F42_FLAGS=f43 selects the football under test (the studies print which one ran).
cargo run --release --example f43_profile -- 600 f43               # fouls, cards, penalties, shot buckets, workload by line and activity; modes m12|f42|f43
cargo run --release --example f43_hotpaths -- 8                    # seconds a match by phase
F42=1 cargo run --release --example f4_perf                        # also F43=1; M12 by default
cargo run --release --example m9_penalty -- --n 300 --mode f43     # penalties against the production configuration
F42_FLAGS=f43 F43_SEEDS=300 cargo test --release --test f43_validation -- --ignored --nocapture quick_grid
F42_FLAGS=f43 F43_SEEDS=150 cargo test --release --test f43_validation -- --ignored --nocapture dominance_matrix   # ~35 min
F42_FLAGS=f43 F43_SEEDS=200 cargo test --release --test f43_validation -- --ignored --nocapture pressing_levels
F42_FLAGS=f43 F43_SEEDS=200 cargo test --release --test f43_validation -- --ignored --nocapture quality_beats
F42_FLAGS=f43 F42_SEEDS=300 cargo test --release --test f41_calibration -- --ignored --nocapture report_the_mentality_ladder
F42_FLAGS=f43 F42_SEEDS=300 cargo test --release --test f42_context -- --ignored --nocapture
F42_FLAGS=f43 cargo test --release --test f41_scenarios -- --ignored --nocapture scenario_
cargo test --release --test f43_block_scenarios -- --ignored --nocapture
cargo test --release --test f43_mechanisms; cargo test --release --no-fail-fast
# OFM (src-tauri/, with the local override or the pinned revision)
APFM_WORLD_JSON=<copy of apfm-fm24-2026.json> cargo test --release -p ofm_core --lib fm24_workload -- --ignored --nocapture
APFM_WORLD_JSON=<copy> cargo test --locked --release --lib interactive_season -- --ignored --nocapture --test-threads=1
```

