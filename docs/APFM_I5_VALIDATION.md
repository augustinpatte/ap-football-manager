# APFM I5: validation and limits

What was run for I5, on which commit, with which command, and what it does and does not support. The
design and the decisions are in [`APFM_I5_TACTICAL_INTELLIGENCE.md`](APFM_I5_TACTICAL_INTELLIGENCE.md).

Every number here is **RUN** on the working tree of `deepseek/i5-tactics`, from
`f31dfa2669f68e0025e3668c8553dd4eb116db2a` (I4), unless it is marked otherwise. The I4 baseline was
run on a worktree of that commit with only the study and its helper copied in
(`apfm-i5-baseline`): no production file differs. A study that selects zero tests, fails to compile
or prints no matches is not evidence and is not reported; where the study can assert its sample
(`i5_career`, `i5_typical_readings`) it does before it prints.

Standard errors are over the independent units named in each section - clubs for the formation
studies, worlds for the career and match studies - never over matches.

## 1. The engine rule

| Check | Result |
| --- | --- |
| `engine::group_depth` unit tests: a full group is its plain mean at every weight; a thin group of better players is worth less than its mean; its drawn player is worth less; a thin group of weaker players is worth more; weight 0 is bit-for-bit the plain mean; an empty group is the side's own standard; a full group is four defenders, four midfielders, two forwards | RUN, `cargo test -p engine --lib group_depth`: 7 passed |
| The live engine's four group contests read the group at its depth; a side's lone forward is read at the depth of its front two, and at weight 0 the state reads the plain mean | RUN, `cargo test -p engine --lib`: 72 passed (1 new test at the state seam) |
| The legacy instant engine's shot defence reads its defenders the new way | READ (`engine/src/engine/resolution.rs` passes `group_depth_weight` to `defensive_quality`) |
| The red-card reaction to a sending-off | RUN, `cargo test -p engine --test ai_decision_tests`: 21 passed (3 new) |
| The pre-match response's answers and their constraints | RUN, 9 tests in `ofm_core::matchday_tactics` (`cargo test -p ofm_core --lib matchday_tactics`) |
| A serialized `MatchConfig` written before the field keeps loading: `group_depth_weight` is `#[serde(default)]`, so an absent field reads 0, the plain mean. A career is unaffected: `live_match_manager` builds `MatchConfig::default()` for every match, so a loaded career plays the depth-aware rule | READ (`engine/src/types.rs`, `ofm_core/src/live_match_manager.rs`) |

**What fails without the change.** Four depth tests and both sending-off tests were run red by
removing the rule in a scratch copy, then restored and run green:

- Replacing the depth-filled mean with the plain mean fails
  `a_thin_group_of_the_best_players_is_worth_less_than_its_mean`,
  `the_drawn_player_of_a_thin_group_is_worth_less` and
  `a_thin_group_of_weaker_players_is_worth_more_than_its_mean`, and at the live-match seam
  `a_lone_forward_is_read_at_the_depth_of_his_front_two` reads a lone 90-rated forward at 1.0 of his
  own finishing; the four identity, empty-group and shape tests still pass.
- Short-circuiting the two red-card branches fails
  `a_side_a_man_down_stops_pressing_and_absorbs` and `a_side_a_man_up_pushes_the_advantage`, while
  the discriminator `a_side_with_eleven_men_level_at_the_hour_does_not_absorb` still passes: it is
  the sending-off that calls for a reaction, not the clock.
- `matchday_tactics` is new, so it has no pre-change red run. Its tests name the exact dials of each
  answer (a deep block, a high line, an aggressive press, width, a neutral opponent), and assert
  that no opponent takes a plan past two moves, that no plan breaks the engine's ration of
  under-priced dials, and that an answer the plan already holds is not named.

**What the rule does.** Only the four group contests change: the midfield contest, the possession
contest, the attacking third and the shot (the shooter and the men screening him). The numbers edge
is untouched: how many players stand in a group is still a fact about the shape, and width is still
what a back three buys. At weight 0 every function is the identity, so a configuration that predates
the field plays the engine it was calibrated on, not an approximation of it.

## 2. The formation evaluator, out of sample

`season-harness/tests/p61_formation_selection`, leave one world out:

```text
P6_WORLD=gate P6_SEEDS=1,2,3,4,5,6,7,8,9,10,11,12 P61_CLUBS=16 P61_REPS=8 \
  cargo test --release -p season-harness --test p61_formation_selection -- --ignored --nocapture
```

192 clubs in 12 worlds, every club in all eight formations, each against a fixed panel of six
opponents home and away, eight repetitions: 1,536 club-formation cells, 12 worlds held out in turn.
A club's points a match vary across its shapes with **sd 0.166**; the split-half correlation of two
independent halves of its matches is **+0.318 (se 0.026)**, the ceiling any score can reach.

| Candidate | Correlation with points | Gain over a random shape | Best - worst shape | Chooses (when it must) |
| --- | --- | --- | --- | --- |
| squad model's score, frozen (what I4 used) | -0.061 (0.029) | -0.025 (0.013) | -0.044 (0.018) | 4-3-3 129, 4-1-4-1 23, 5-3-2 18 |
| squad model's score, refitted per world (its own sign) | +0.061 (0.029) | +0.020 (0.013) | +0.046 (0.019) | 3-4-3 130, 4-1-4-1 21, 3-5-2 23 |
| mean stored overall of the eleven | +0.031 (0.031) | +0.000 (0.013) | +0.007 (0.021) | spread |
| the shape alone (7 dummies) | +0.231 (0.026) | +0.072 (0.012) | +0.122 (0.019) | 3-5-2 192 |
| own contest means (8) | +0.305 (0.032) | +0.076 (0.012) | +0.158 (0.020) | spread |
| fielded back line + finishing only (2) | +0.289 (0.032) | +0.069 (0.012) | +0.152 (0.019) | spread |
| **I5: depth-aware, three groups (refit)** | **+0.210 (0.033)** | **+0.072 (0.013)** | +0.135 (0.020) | 4-4-2 67, 3-5-2 36, 4-1-4-1 32 |
| I5: depth-aware, two groups | +0.197 (0.032) | +0.068 (0.012) | +0.119 (0.019) | 4-4-2 65, 3-5-2 45 |
| I5: suitability form, three groups | +0.172 (0.034) | +0.051 (0.013) | +0.098 (0.021) | spread |
| I5: suitability form, two groups | +0.163 (0.034) | +0.056 (0.013) | +0.107 (0.020) | spread |
| I5: cover alone (slots with no natural player) | -0.000 (NaN) | -0.018 (0.012) | -0.062 (0.018) | 4-1-4-1 **191 of 192** |
| I5: depth-aware, two groups + cover | +0.197 (0.032) | +0.068 (0.012) | +0.119 (0.019) | identical to the two-group row |
| shape + back line + finishing (9) | +0.354 (0.027) | +0.093 (0.012) | +0.188 (0.018) | 3-5-2 112, 4-4-2 29 |
| shape + own contest means (15) | +0.363 (0.027) | +0.105 (0.012) | +0.198 (0.019) | 3-5-2 102, 4-4-2 37 |
| **I5: the shipped rule, weights frozen** | **+0.210 (0.033)** | **+0.075 (0.013)** | +0.133 (0.020) | 4-4-2 72, 3-5-2 39, 4-1-4-1 29, 4-2-3-1 17, 5-3-2 16, 4-3-3 11, 3-4-3 8 |

The "chooses" column is what the score names if it has to pick one of the club's eight shapes for
every club in the sample; "spread" means no shape was named for more than a fifth of the clubs. A
constant predictor (cover alone) has no correlation, which is the NaN.

**Fitted weights** (goals of goal difference a match per rating point, fitted on every club, out of
sample by world):

| Form | Back line | Midfield | Finishing |
| --- | --- | --- | --- |
| shipped (the first full study) | +0.022 | +0.010 | +0.027 |
| depth-aware, three groups, refit this run | +0.021 | +0.014 | +0.024 |
| suitability, three groups, refit this run | +0.020 | +0.009 | +0.025 |
| depth-aware, two groups, refit this run | +0.021 | - | +0.024 |
| fielded back line + finishing, refit this run | +0.029 | - | +0.017 |

**What this supports.**

- The rule I4 used is worse than a coin toss at ranking a club's own shapes: **-0.061**, and its
  favourite earns **-0.025 points a match**. Its favourite is 4-3-3 for 129 of 192 clubs, which is
  where the old league's 4-3-3 concentration came from. The refit row is the same score with the
  weight the data chose, and that weight is negative (-0.029): a correlation is unchanged by a
  positive rescaling, so the magnitude repeats with the opposite sign. Neither the score nor its
  reverse is a stable ranking, which is the finding.
- The engine's reading ranks them at **+0.210** frozen, two thirds of the split-half ceiling, and its
  favourite earns **+0.075 points a match** over a random shape. The refit and the frozen rule print
  the same correlation (+0.210 to three decimals) and choose a different shape for 9 of the 192
  clubs: the shipped weights are not a lucky draw, and they are measured as shipped.
- The **suitability** form (P6.1's) is worse in this engine (**+0.172** against **+0.210**). The gap
  is inside one combined standard error, so the measurement alone does not settle it; the decision
  was taken on the causal reading too: once a thin group is read at its depth the level of a shape
  is the engine's own price, so normalising it away throws away signal the engine charges for.
- **Midfield** is priced positive in both fits (`+0.010`, `+0.014`), and it is the smallest of the
  three. The two-group reading reaches `+0.197`, so midfield is worth about 0.013 of correlation on
  this sample; it is kept because it is positive, it is a group the engine reads, and dropping it
  would be a simpler story than the evidence.
- **Cover** is not a formation criterion: alone it sends 191 of 192 clubs to 4-1-4-1, and added to
  the reading it changes no choice (the chosen distributions are identical) and earns nothing. It is
  left out of the score and reported here, not shipped as a rule.
- The harder candidates (`shape + ...`, 9 and 15 predictors) reach higher correlations, but most of
  what they add is the league's average points per shape refitted on the training worlds, which is
  not a decision a club can make about its own squad. The three-group reading gets within 0.02 of
  them with three numbers.

**What it does not support.** These are within-club contrasts over one season's worth of matches per
repetition, on generated worlds, with every player fit at kick-off: they say which score ranks a
club's shapes, not how a ten-season career with injuries, transfers and ageing turns out. Section 5
measures the career.

## 3. What a typical squad reads in each shape

`season-harness/tests/i5_typical_readings`, 192 clubs in 12 worlds, full-strength eleven,
depth-aware:

| Shape | Back line | Midfield | Finishing |
| --- | --- | --- | --- |
| 4-4-2 | 71.039 (0.28) | 71.238 (0.31) | 73.253 (0.44) |
| 4-3-3 | 70.993 (0.28) | 70.408 (0.27) | 68.846 (0.41) |
| 3-5-2 | 70.630 (0.26) | 71.474 (0.27) | 72.497 (0.41) |
| 4-5-1 | 70.914 (0.28) | 70.807 (0.26) | 71.619 (0.36) |
| 4-2-3-1 | 70.914 (0.28) | 70.807 (0.26) | 71.619 (0.36) |
| 3-4-3 | 70.631 (0.26) | 71.167 (0.29) | 69.469 (0.43) |
| 5-3-2 | 69.985 (0.26) | 70.427 (0.27) | 72.544 (0.43) |
| 4-1-4-1 | 70.959 (0.28) | 71.105 (0.25) | 71.823 (0.37) |

Two claims, no more:

- **The level of a shape is a real property of the engine.** The back line and the midfield sit
  within about a point of each other in every shape, while finishing spreads over 4.4 points:
  the shape decides which of a squad's forwards stand in the finishing group, and the engine reads
  them. That is why the absolute reading has something to read, and why a typical squad's 4-4-2
  reads 4.4 finishing points better than its 4-3-3: the front two are the second-best of the
  squad's forwards, the front three include the third-best.
- **The depth rule moves the shapes whose groups are short of a full one.** A back three's hold is
  read as if the fourth defender were an average outfield player of the same side (70.63 against the
  4-4-2's 71.04), and a lone forward's finishing is read against a second forward (5-3-2's front two
  is full; 4-5-1's and 4-2-3-1's lone forward is filled to two, and reads 71.62).

These are population levels for a typical squad, not a club's own reading; the evaluator computes
the club's own eleven and does not use these numbers at run time.

## 4. How steady the rule is

`season-harness/tests/p61_formation_stability`, 3 worlds x 17 weekly reviews, formations held:

```text
P6_WORLD=gate P6_SEEDS=1,2,3 P61_WEEKS=17 \
  cargo test --release -p season-harness --test p61_formation_stability -- --ignored --nocapture
```

| Rule | Best shape changes week to week | Gap best - current: mean / median / p90 | Gap clears the margin |
| --- | --- | --- | --- |
| squad model + taste (I4) | 1.4 % | 0.96 / 0.87 / 1.84 | 59.5 % of club-weeks |
| engine fit + taste (I5) | 1.2 % | 0.33 / 0.11 / 1.00 | 18.9 % of club-weeks |

The new rule is slightly **less** volatile week to week than the old one, and the gaps it sees are
smaller: the engine reads a club's own eight shapes as closer together than the squad model did. The
base change margin is unchanged at 0.75 (the study's mean realized margin is 0.72, since a new
manager's mandate and a worrying run reduce it), so a shape change needs a club's best shape to be
nearly a full rating point better than the one it plays. The career counts a change when a club's season-end shape
differs from its previous season-end shape (section 5), so the two measures are not the same: the
weekly study says a settled club's best shape rarely moves, and the career says more clubs arrive at
a different shape between seasons.

## 5. A career

`season-harness/tests/i5_career`, three worlds in the gate configuration, five measured seasons each
(season 1 is the warm-up), the same worlds and seeds on both trees:

```text
P6_WORLD=gate P6_SEEDS=1,2,3 I5_SEASONS=5 \
  cargo test --release -p season-harness --test i5_career a_career_season_by_season -- --ignored --nocapture
```

| A career, mean over 3 worlds x 5 seasons | I4 baseline | I5 |
| --- | --- | --- |
| goals a match | 2.501 (0.127) | 2.609 (0.117) |
| shots a match | 25.642 (0.210) | 25.203 (0.107) |
| on target a match | 7.378 (0.096) | 7.422 (0.107) |
| home possession % | 51.684 (0.055) | 51.754 (0.015) |
| draws a match | 0.261 (0.012) | 0.250 (0.007) |
| home points a match | 1.484 (0.020) | 1.486 (0.011) |
| home minus away goals | 0.182 (0.031) | 0.174 (0.006) |
| red cards a match | 0.102 (0.006) | 0.104 (0.005) |
| clubs changing shape a season | 10.267 (0.176) | 22.267 (1.485) |
| formations fielded in the league | 5.600 (0.346) | 7.800 (0.115) |
| commonest shape share % | 51.224 (1.471) | 51.224 (0.995) |
| distinct tactical plans a season | 28.133 (0.481) | 27.667 (0.769) |
| seconds a season | 10.560 (0.049) | 10.511 (0.110) |

The shapes those club-seasons were played in, 1,422 on each tree:

| Shape | I4 | I5 |
| --- | --- | --- |
| 4-4-2 | 11.0 % | 52.5 % |
| 3-5-2 | 3.2 % | 17.4 % |
| 4-1-4-1 | 13.0 % | 10.5 % |
| 5-3-2 | 18.7 % | 4.9 % |
| 4-3-3 | 52.5 % | 4.5 % |
| 4-5-1 | 0.7 % | 3.6 % |
| 4-2-3-1 | 0.9 % | 3.5 % |
| 3-4-3 | 0.0 % | 3.2 % |

**What moved, and what did not.**

- **The football did not move.** Goals, shots, shots on target, possession, draws, home points, home
  advantage and red cards all sit within two standard errors of the I4 career (the largest is shots,
  -0.44 at about 1.9). The goal rate is
  +0.108 a match, against a combined standard error of about 0.17: not a claim that I5 scores more,
  and nowhere near the +15-20 % the disabled evaluator produced in P6.1.
- **The decision moved.** Twice as many clubs change shape between seasons (22.3 against 10.3), the
  league fields 7.8 of the 8 shapes rather than 5.6, and the identity changes from 4-3-3 (52.5 % of
  club-seasons) to 4-4-2 (52.5 %), with 3-5-2 rising from 3.2 % to 17.4 % and 4-3-3 falling to
  4.5 %.
- **Why 4-4-2.** It is the engine's price of a typical squad (section 3): a front two is the
  squad's two best finishers, a front three includes the third. That is what the depth-aware reading
  exposes, and the fit charges for it. It is not a quota and not the old score in disguise: 4-3-3
  still wins 4.5 % of club-seasons, 3-5-2 17.4 %, 5-3-2 4.9 %, and the generated worlds give most
  clubs a similar spread of finishing, so the same shape winning for most of them is a consequence
  of the squads, not a target. The concentration (51 % in the commonest shape) is the same as I4's;
  its cause is different, and it is now the engine's own reading rather than a rating of players the
  engine never sees.
- **No oscillation.** 22.3 of a league's clubs change shape in a season, not 22.3 times a club
  flips: the weekly study (section 4) has the best shape changing 1.2 % of weeks, and the base change
  margin is unchanged at 0.75.

**A longer career.** Two worlds x ten measured seasons, same command with `I5_SEASONS=10` and
`P6_SEEDS=1,2`:

| Long career, mean over 2 worlds x 10 seasons | I5 |
| --- | --- |
| goals a match | 2.522 (0.123) |
| shots a match | 25.553 (0.084) |
| on target a match | 7.538 (0.096) |
| home possession % | 51.808 (0.037) |
| draws a match | 0.252 (0.003) |
| home points a match | 1.478 (0.000) |
| home minus away goals | 0.168 (0.003) |
| red cards a match | 0.100 (0.002) |
| clubs changing shape a season | 21.350 (0.450) |
| formations fielded in the league | 7.750 (0.050) |
| commonest shape share % | 50.506 (2.405) |
| seconds a season | 11.362 (0.037) |

Shapes over 1,738 club-seasons: 4-4-2 51.3 %, 3-5-2 17.5 %, 4-1-4-1 12.7 %, 5-3-2 5.3 %,
4-3-3 4.3 %, 3-4-3 3.2 %, 4-5-1 3.0 %, 4-2-3-1 2.6 %.

Ten seasons do not drift: the goal rate is 2.522 against the five-season run's 2.609 (inside the
standard error), shape changes 21.4 against 22.3, the commonest share 50.5 % against 51.2 %, and
every other column is the same football. The AI does not herd into one shape as its squads age or as
managers change.

## 6. What a manager does in a match

`season-harness/tests/i5_career`, 3 worlds x 8 clubs, 60 warm-up days, the same worlds and seeds on
both trees:

```text
P6_WORLD=gate P6_SEEDS=1,2,3 P61_CLUBS=8 P6_WARMUP=60 \
  cargo test --release -p season-harness --test i5_career what_a_manager_does_during_a_match -- --ignored --nocapture
```

| A match | I4 baseline | I5 |
| --- | --- | --- |
| substitutions a match | 3.358 (0.165) | 3.486 (0.127) |
| goals after the hour | 1.250 (0.064) | 1.330 (0.014) |
| shots after the hour | 6.514 (0.070) | 6.604 (0.094) |
| matches with a sending-off | 0.128 (0.015) | 0.101 (0.007) |
| matches with a leader at the hour | 0.705 (0.013) | 0.736 (0.041) |
| leads held to the end | 0.828 (0.007) | 0.805 (0.035) |

The bench is used a little more, slightly more matches are still level at the hour, and lead
protection is a touch lower; every difference is within about one and a half standard errors of the
comparison, and none is significant at two. This is what the match-management change is
expected to look like from outside: the reaction to a sending-off is a decision, not a goal. What
the study does support is that the reaction does not degrade the match: the same number of goals,
shots and leads to protect, and no collapse in leads held (0.805 against 0.828, within the noise).

Section 1 records the targeted tests for the two new sending-off branches. The pre-match response
is tested at its seam (9 tests in `matchday_tactics`, and the career applies it to every AI side
before kick-off); its own contribution to results is not isolated here (section 8).

## 7. Five-zone and M12

M12 (`apfm-match-lab`, pinned rev `28f34b10d4fe7cadacc0030acaaa0cd1aa9fdf5f`) is an external
repository. Both engines are handed the same `PlayStyle` and the same `TacticsPhaseSettings`;
`ofm_core::spatial_adapter::tactics` maps every one of the nine dials, the play style and the four
phase groups onto M12's own scales (the mapping is in the design document, section 5).

`season-harness/tests/p61_dials_both_engines`: two clubs forced to identical players, one setting
moved against a neutral 4-4-2, home and away, the same seeds in both engines (1,500 five-zone
matches, 60 M12 matches; M12 costs about a second a match):

```text
P6_WORLD=gate P61_MATCHES=1500 P61_M12_MATCHES=60 \
  cargo test --release -p season-harness --test p61_dials_both_engines -- --ignored --nocapture
```

Change from the subject's own neutral baseline:

| Setting | Five-zone: points / shots / possession % | M12: points / shots (se) / possession % | Sign of the shot effect |
| --- | --- | --- | --- |
| Defensive | +0.001 / -0.14 / +0.08 | +0.017 / **-2.05** (0.61) / +1.25 | agree |
| Attacking | +0.081 / +0.32 / +0.51 | +0.300 / -0.30 (0.63) / -2.31 | opposite, M12 not resolvable |
| Possession | +0.115 / +0.51 / +2.10 | +0.033 / -0.35 (0.60) / -0.52 | opposite, M12 not resolvable |
| Counter | +0.069 / +0.06 / -1.57 | +0.650 / +0.43 (0.67) / +1.07 | same sign, neither resolvable |
| High press | +0.000 / -0.10 / -0.06 | +0.100 / -0.50 (0.58) / -0.85 | same sign |
| Very low line | +0.102 / +0.01 / -1.96 | +0.317 / **+3.00** (0.68) / +9.80 | five-zone flat, M12 strongly positive |
| High line | -0.020 / +0.27 / +0.70 | +0.367 / **-1.23** (0.60) / -3.28 | opposite; M12 two standard errors |
| Passive press | -0.012 / +0.20 / -0.29 | +0.300 / +1.08 (0.75) / +2.31 | same sign |
| Aggressive press | +0.047 / -0.23 / +0.58 | +0.183 / **-1.63** (0.52) / -2.37 | same sign |
| Narrow | +0.112 / +0.36 / +0.10 | +0.450 / +1.00 (0.63) / +0.21 | same sign |
| Wide | -0.041 / -0.29 / +0.10 | +0.300 / +0.42 (0.52) / +0.64 | opposite, neither resolvable |
| Patient tempo | +0.026 / -0.15 / +0.20 | +0.483 / -0.18 (0.63) / -0.41 | same sign, both small |

**What this supports.**

- **Seven of the twelve dials push shots the same way in both engines** (Defensive, Counter, High
  press, Passive, Aggressive, Narrow, Patient). Where the two disagree in sign (Attacking,
  Possession, Wide), M12's own shots standard error is larger than the effect: those are not
  evidence of a different direction, they are matches that did not resolve one. The high line and
  the very low line are the two real divergences, and both are the ones P6.1 already documented:
  M12 prices the very low line as a strong positive (+3.00 shots) where the five-zone engine prices
  it flat (+0.01), and the high line as a strong negative (-1.23) where the five-zone engine prices
  it slightly positive (+0.27).
- **Possession diverges in the same places P6.1 recorded** (`docs/APFM_P6_1_TACTICAL_ALIGNMENT.md`
  section 5.2): Possession (+2.10 against -0.52), Counter (-1.57 against +1.07), Very low line
  (-1.96 against +9.80), High line (+0.70 against -3.28), Aggressive press (+0.58 against -2.37).
  The five-zone engine prices possession through its midfield contest; M12 prices it through
  geometry. I5 changed neither scale and neither mechanic.
- **M12 prices the dials it resolves much more strongly on shots**: Defensive -2.05 against -0.14
  (3.4 of M12's own standard errors) and Aggressive press -1.63 against -0.23 (3.1), the same
  direction at a different magnitude, as P6.1 measured. The other dials' point estimates are larger
  in M12 but not resolved at this sample. I5 did not re-scale a dial to make the two agree: the
  shared vocabulary is the nine dials the game already exposes, and a change to M12's scales would be
  a change to a pinned external dependency.

**What remains open.** M12's standard errors here (0.5-0.75 shots) make anything smaller than about
1.5 shots unresolvable in 60 matches, and the five-zone side has 25 times the matches. A stronger
statement about the high line and the very low line needs thousands of M12 matches over several club
pairs, which is P6.1's own conclusion and is not attempted here.

**AI-assigned roles.** A separate paired study (`p61_world_roles`) clears every AI-assigned role
(`P61_NOROLES=1`) and plays the same worlds and seeds:

```text
P6_WORLD=gate P6_SEEDS=1,2,3 P61_REPS=2 cargo test --release -p season-harness --test p61_world_roles -- --ignored --nocapture
```

| 3 worlds x 2 repetitions | Roles on | Roles cleared |
| --- | --- | --- |
| goals a match | 3.510 (0.026) | 3.431 (0.031) |
| shots a match | 25.184 (0.285) | 24.805 (0.240) |
| on target a match | 7.970 (0.200) | 7.861 (0.205) |
| home possession % | 51.743 (0.070) | 51.873 (0.073) |
| draws a match | 0.217 (0.006) | 0.203 (0.004) |
| home points a match | 1.543 (0.013) | 1.575 (0.027) |

The AI fills roles across its squads (26 role types, 0.63 poachers and 0.55 carrileros a squad among
them), and clearing them all moves the goal rate by **+0.079 a match**, about 2.3 %: larger than
either world-level standard error, and the only column that moves that far. Shots (+0.38) and shots
on target (+0.11) have the same sign but are not resolvable at this sample; possession does not move.
Roles are therefore a small real effect, not decoration, and nowhere near the 15-20 % goal inflation
the disabled evaluator produced. No role, role weight or assignment rule was changed by I5.

## 8. Determinism, cost and limits

**Determinism.** Two identical runs of the selection study (3 worlds, 4 clubs, 2 repetitions)
produced identical measurement lines; only the wall-clock line differed. The five-season and
ten-season careers replay the same worlds identically for their shared seasons (for example world 1,
season 2: 2.547 goals, 24.89 shots, 7.14 on target, 51.52 % possession). Nothing in the new rules
reads a random number: the depth factors are pure functions of the players on the pitch, the
match-day response is a pure function of the two plans and the opponent's style, and the red-card
branches read the score, the clock and the men on the pitch.

**Cost.** The career costs 10.51 seconds a season against 10.56 on I4, and 11.36 seconds a season
over ten seasons: no measurable cost. The depth factors are computed when a contest reads a group,
from the players on the pitch, with no per-minute full-squad recomputation; at weight 0
`group_depth_factor` returns exactly 1.0 before doing any work. The formation review is weekly and
unchanged in cost; the match-day response runs once, before kick-off. The out-of-sample selection
study (1,536 club-formation cells, 8 repetitions, 12 held-out worlds) runs in 211 seconds.

**Limits.**

- The formation study plays generated worlds with identical panels and fit squads; it measures a
  ranking, not a career. The career study is the check on the career, and it is bounded: 3 worlds x
  5 seasons against I4, and 2 worlds x 10 seasons for drift.
- The I4 and I5 careers share worlds and seeds but not matches, because the AI's decisions differ.
  A difference smaller than about 0.15 goals a match is not resolvable in 3 worlds, and the report
  does not claim one.
- The pre-match response's own contribution to results is not isolated. Its dials have measured
  prices (this section), its seam and constraints are tested, and the career shows the system as a
  whole; a paired-seed estimate of the response bundle on the same fixtures (with and without) is
  not in this phase.
- M12 is a pinned external dependency: only the sign and order of effects can be compared, its
  60-match standard errors leave many dials unresolved, and the known high-line and very-low-line
  divergences are not defects I5 can fix from this repository.
- The roles study is 3 worlds x 2 repetitions: it supports a small positive effect on goals, not a
  per-role claim, and I5 did not touch roles.
- `engine::simulate` (the legacy instant engine, used by the simulation lab) reads its defenders at
  their depth but does not apply the other contest factors; it is not what a career plays.
- No production frontend change was made: the nine dials were already exposed, and I5 adds no
  instruction a player cannot set and no string a player can read.
