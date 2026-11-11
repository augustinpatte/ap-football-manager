# APFM P6: match realism and tactical consequences

Branch `claude/p6-match-realism`, from `ea469f3a` (I1: P5 + D7B2). **Not pushed, not merged.** Financial work is in
[`APFM_P6_FINANCIAL_INTEGRITY.md`](APFM_P6_FINANCIAL_INTEGRITY.md); evidence, gates and limits are in
[`APFM_P6_VALIDATION.md`](APFM_P6_VALIDATION.md). M12 (`apfm-match-lab`, rev `28f34b10`), the registry files and the D7B3 worktree were not touched.

Evidence labels: **RUN** (executed this phase), **READ** (read in source). Standard errors are over clubs or matches as stated.

## 1. What was found and what changed

| # | Finding | Evidence | Change |
| --- | --- | --- | --- |
| 1 | With **identical players** the five-zone engine priced every formation at exactly **nothing**: all eight shapes took 1.340 points a match, bit for bit. A shape was worth only who happened to be drawn into a group | RUN, 8 × 8 shapes, 2,000 matches a cell | Numbers are priced (below) |
| 2 | A **sending-off cost 0.00 points** a match (equal players, a midfielder out before kick-off, 800 matches). Group ratings are means and the contests draw one player | RUN | Numbers are priced; a man down now costs > 0.08 (test) |
| 3 | The **defensive half of every play style never fired.** `play_style_modifier(.., is_own_phase = false)` returns 1.0, and the attacking-third contest asked the *defender's* style with `false`. A Defensive side was no better at defending (−0.046 points, the worst style), an Attacking one never paid for its attack (+0.058) | RUN; test red before, green after | Fixed: the defender's own phase is applied |
| 4 | Dials that paid with no cost. At equal players: possession +0.086, a very low line +0.095, narrow width +0.061, counter-press +0.046; **stacked, +0.23 to +0.27 points a match** (about 9 league points a season) | RUN, 6,000 matches each | Rebalanced; stacked now +0.08 to +0.11 |
| 5 | **M12 seated every formation's starters on the wrong geometry slots** (4 to 6 of 11 per formation; a left back on the right, central roles shuffled) | RUN, test red before | Fixed in the adapter (section 5) |
| 6 | The squad model's formation score does not predict results. Not a defect of the engine: see section 4 | RUN | Explained; no change |
| 7 | The match-day formation adjustment (disabled in P5) re-measured on the new engine: still not demonstrably better than the club's own shape | RUN, 12 worlds | **Stays disabled** |

## 2. Baseline and commits

Baseline: P5's measured figures (I1 reproduced them digit for digit; the engine is untouched by the merge). P5 reference: 3.18 goals a match, 22.4 % draws, possession style +0.125, very low line +0.070, squad-model slope −0.021 ± 0.021.

| Commit | Change |
| --- | --- |
| `7e545054` | `test(p6)`: `match_study` (one pairing through either engine) and two measurement studies |
| `5c2933f4` | `feat(engine)`: numbers priced; a style's defence applied; dials rebalanced |
| `a68b2526` | `test(p6)`: studies for the M12 fit, what predicts a shape, goal-rate gap, misfit cost |
| `3bd7364d` | `fix(m12)`: each starter on the geometry slot of his own seat |

(Finance commits are listed in the finance document.)

## 3. The five-zone engine

### 3.1 Numbers in the contests

`MatchConfig` gains `midfield_numbers_weight` and `attack_numbers_weight` (shipped **0.3 / 0.3**; `#[serde(default)]` = 0, so an older configuration reads as before, and **a weight of 0 is bit-for-bit the old engine**: a test pins it).

- **Midfield contest and the per-minute possession contest:** the possessor's strength scales by `(own midfielders on the pitch / opposing midfielders)^w`, ratio clamped to [0.5, 2].
- **Attacking-third contest:** the attacker scales by `(own forwards / opposing defenders ÷ 0.5)^w` (level at a 4-4-2's two to four).
- A player sent off leaves the count. Injured players play on (as in P5; the engine has no rule for a hurt player).

Equal players, one formation against a 4-4-2 (RUN):

| | before | weights 0.3 / 0.3 (row mean points a match over all eight opponents) |
| --- | --- | --- |
| 4-4-2 / 4-3-3 / 3-5-2 / 5-3-2 | 1.340 each | 1.358 / 1.324 / 1.381 / 1.317 |
| 4-5-1 (= 4-2-3-1 = 4-1-4-1 to this engine) / 3-4-3 | 1.340 | 1.365 / 1.393 |

Spread 0.077 points a match from best to worst shape: of the order of P5's measured ±0.09 between shapes with real squads, and **no shape dominates**. Direction: more forwards per defender shoot more (`forwards_per_defender_decide_who_gets_the_shots`), more midfielders keep more of the ball, and a side down a man loses ground.

**What the weights are not.** They were *not* calibrated to M12: see 3.2.

### 3.2 M12 against the five-zone engine, at equal players (RUN)

Two clubs forced to identical players (rating 70), each formation against a 4-4-2, home and away, in both engines (M12 500 matches a row, standard error ≈ 0.06; five-zone 2,000):

| Shape | M12 points | M12 shots | M12 possession | Five-zone points (before) |
| --- | --- | --- | --- | --- |
| 4-4-2 | 1.394 | 11.9 | 50.7 | 1.359 |
| 4-3-3 | **1.514** | 12.4 | 50.3 | 1.394 |
| 3-5-2 | 1.370 | 11.6 | 50.7 | 1.395 |
| 5-3-2 | **1.232** | 10.7 | 49.0 | 1.382 |
| 4-5-1 / 4-2-3-1 / 4-1-4-1 | 1.492 / 1.364 / 1.316 | 13.5 / 12.6 / 12.0 | 52.3 / 50.4 / 48.9 | 1.382 |
| 3-4-3 | **1.620** | 13.8 | 52.4 | 1.387 |

M12 prices shape strongly (3-4-3 +0.23, 5-3-2 −0.16 against its own 4-4-2) and **prices slots the five-zone engine cannot see**: 4-5-1, 4-2-3-1 and 4-1-4-1 are one shape to four group counts and three in M12 (spread 0.18). A grid fit of the two weights to M12's shape effects (points, shots, possession, 3,000 matches a shape) found the best pair, 0.2 / 0.2, at a misfit of 38.8 against 45.0 for pricing nothing. **Group counts explain almost none of M12's shape effect**; its geometry (width, wing-back depth) does. The shipped 0.3 / 0.3 is therefore justified by the sending-off and the zero price, not by M12: **the two engines still price formations differently**, and this phase does not claim otherwise.

### 3.3 Styles and dials

Equal players, one setting moved, points a match added (6,000 matches, paired seeds, standard error ≈ 0.012):

| Setting | before | after |
| --- | --- | --- |
| style Attacking / Defensive | +0.058 / −0.046 | **+0.012 / +0.003** |
| style Possession | +0.086 | +0.053 |
| line VeryLow / Low / High | +0.095 / +0.051 / −0.077 | **+0.006 / +0.016 / −0.027** |
| width Narrow / Wide | +0.061 / −0.031 | +0.025 / −0.026 |
| counter-press Long | +0.046 | +0.044 |
| **stack: possession + very low + narrow** | **+0.229** | **+0.083** |
| **stack: attacking + very low + narrow + long counter-press** | **+0.269** | **+0.107** |

Changes: the defender's style phase is applied (fix 3); possession's attack penalty 0.97 → 0.93 and midfield 1.15 → 1.12; attacking's attack 1.12 → 1.10; shot-conversion by line High 1.12 → 1.05, Low 0.92 → 0.975, VeryLow 0.85 → 0.95; width against a normal shape: narrow 1.02 → 1.08, wide 0.97 → 0.95 (narrow against compact 1.05 → 1.08); counter-press exposure Short 0.05 → 0.06, Long 0.11 → 0.14. Every one is a coefficient moved to remove a measured free gain, not a new bonus. **Remaining:** possession (+0.053) and counter-press (+0.044) are still the largest single dials and the best stack is still +0.08 to +0.11.

### 3.4 On real squads (RUN, 3 worlds × 8 clubs × 6 panel × 8 reps, 18,432 matches, warmed 60 days)

| | P5 | P6 |
| --- | --- | --- |
| Goals a match | 3.18 | **3.12** |
| Draws / 0-0 / five or more goals | 22.4 / 4.5 / 21.9 % | 22.1 / 5.0 / 21.0 % |
| Subject win / draw / loss | 42.6 / 22.4 / 35.0 | 41.0 / 22.1 / 36.8 |
| Possession style (ppg ± se over clubs) | **+0.125** (±0.029) | **+0.018** (±0.020) |
| Very low line | **+0.070** (±0.024) | **−0.053** (±0.025) |
| Attacking / Defensive style | +0.054 / within ±0.04 | −0.049 (±0.020) / −0.020 (±0.021) |
| Formation spread (best, worst) | 3-5-2 +0.086, 4-3-3 −0.082 | 3-5-2 +0.083, 4-5-1 −0.063 |
| A club's ppg varies across its eight shapes by | 0.185 ± 0.015 | 0.185 ± 0.018 |

No dial is above +0.02; the goal rate is unchanged. A deep line is now a small cost against real squads, which is a trade, not a gift.

## 4. Why formation suitability has almost no predictive power (RUN)

1. **It is not that the engine ignores fit.** The same eleven, once seated by their attributes (shooters up front, defenders at the back) and once with the two groups' players in each other's places (a 30-point swing in the attributes the contests read): **1.883 against 0.841 points a match**, a cost of 1.04.
2. **The squad model's shape-to-shape variation is large**, not small: within a club the standard deviation of its score over the eight shapes is **0.71 rating points** (range 2.18; 237 clubs). At the price seen in 1 that would be a visible slope.
3. **Yet the slope is nil:** −0.014 ± 0.029 points a game per rating point (24 clubs × 8 shapes; P5: −0.021 ± 0.021). The squad model's *positional-fit* score is not what the engine reads. The engine's own team ratings do slightly better: defence rating +0.049 ± 0.026, attack +0.001 ± 0.009, midfield +0.011 ± 0.011, the sum +0.009 ± 0.005. None is significant at 24 clubs, but only the squad model's score carries no signal at all.

**Conclusion:** formation choice by the squad model is choice by a number the engine does not read. An engine-aligned score (above all the defence rating) is the candidate for the next phase; it was not built here because 24 clubs cannot distinguish a candidate that is worth +0.05 a point from none.

## 5. Formations seen consistently by both engines

The eleven a club names is slot-aligned to the five-zone list of seats (`player_rating::formation_slots`). The M12 adapter paired starter `i` with the geometry slot `i` of `formation_geometry`. **The two lists disagree in all eight formations** (RUN):

| Formation | slots in a different order | | Formation | slots in a different order |
| --- | --- | --- | --- | --- |
| 4-4-2 | 4 (both flanks, back and midfield) | | 3-4-3 | 4 |
| 4-3-3 | 6 | | 5-3-2 | 4 |
| 3-5-2 | 4 | | 4-5-1 | 6 |
| 4-2-3-1 | 5 | | 4-1-4-1 | 6 |

A left back stood at right back, and in 4-2-3-1 and 4-1-4-1 a seat named central midfielder took a defensive-midfield slot (and four "attacking midfielders" took RM, CM, CM, LM). This was the match a person watches (and the replay viewer). The flank mirror is symmetric in play; the role shuffle is not, and the viewer showed the wrong player in the wrong place.

**Fix** (`spatial_adapter::seat_in_geometry`): each starter takes the geometry slot of his own seat; a seat with none (a five-zone left midfielder where M12 lays out a left wing back) takes the nearest slot on its own flank, a central one the nearest of its group to the middle; exact matches first. The out-of-possession layout is seated the same way. Tests (`every_starter_stands_on_the_geometry_slot_of_his_own_seat`, `a_seat_with_no_slot_of_its_name_takes_the_nearest_on_its_own_flank`): red without it (the left back's base y = 10 instead of 58).
**Group counts**, compared by the row each slot stands on, agree between the two engines for all eight formations (`formation_engine_consistency`).

## 6. The two engines' goal rate (RUN)

The same 300 fixtures of warmed gate worlds (seeds 1, 2) through both engines:

| | five-zone | M12 |
| --- | --- | --- |
| Goals a match (se) | 2.86 (0.11) | **2.65** (0.10) |
| Shots / on target | 24.9 / 7.5 | 25.2 / **11.4** |
| Draws | 25.3 % | 27.0 % |
| Home points | 1.45 | **1.23** |
| Goals when the home side is the stronger third | 2.74 | **2.25** |

The remaining gap between the football a person watches and the one a career is played with is **about 0.2 goals a match, 1.4 standard errors**: not demonstrated. The differences that are visible are M12's greater share of shots on target, a much weaker home advantage (a weaker home third takes 0.69 points in M12, 1.10 in the five-zone engine) and fewer goals when the stronger side is at home. The dormant tier (outside the followed scope) scores 2.4-2.5 and was not changed. Nothing was tuned to close a gap that is within noise.

## 7. The match-day formation adjustment

`matchday_formation` (club's shape for a day when players are missing) re-measured on the P6 engine, twelve worlds, 96 club-scenarios a case:

| Two best out | rule − club's own shape (ppg ± se) | P5 |
| --- | --- | --- |
| defenders | +0.012 ± 0.016 | −0.004 ± 0.015 |
| midfielders | +0.003 ± 0.016 | −0.013 ± 0.015 |
| forwards | +0.022 ± 0.019 | +0.043 ± 0.018 |

All three are under two standard errors. **Not enabled**, and the module's doc comment now records both measurements. Enabling it needs an engine, or an engine-aligned score (section 4), that makes a shape's fit measurable.

## 8. Limits and what is left

1. The five-zone engine reads four groups; M12 reads geometry. The two price formations differently (section 3.2) and **the numbers weights are not M12-calibrated.**
2. Possession (+0.053) and counter-press (+0.044) remain the largest single dials at equal players; the best stack is +0.08 to +0.11, not zero.
3. Roles: the role table is unchanged. Roles that boost a phase the resolvers never ask their position about (AttackingFB, WingBack, ShadowStriker) are still inert, as P5 recorded.
4. `engine::simulate` (the legacy one-shot engine, used only by `sim-bench`) was not given numbers or the style fix and so differs from the live engine. No career match uses it.
5. A hurt player with no substitution left still plays on (P5).
6. M12's own tests were not run; M12 was not modified.
7. **The GUI was not played.** No desktop control exists in this session; nothing here is claimed from the window. The M12 slot fix changes what the viewer shows and is the first thing a person should look at: put a left back at LB in a 4-4-2 and watch him on the left.
