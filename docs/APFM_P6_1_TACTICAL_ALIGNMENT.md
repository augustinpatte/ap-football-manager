# APFM P6.1: tactical intelligence and engine alignment

Branch `claude/p6-1-tactical-alignment`, from `78715a44` (P6). **Not pushed, not merged.** Evidence, gates and limits are in
[`APFM_P6_1_VALIDATION.md`](APFM_P6_1_VALIDATION.md). M12 (`apfm-match-lab`, rev `28f34b10`), the registry, importer, recruitment and
financial code, the shared locale files and the D7B3 worktree were not touched.

Evidence labels: **RUN** (executed this phase), **READ** (read in source). Standard errors are over clubs, worlds or matches as stated.

## 1. What was found and what changed

| # | Finding | Evidence | Change |
| --- | --- | --- | --- |
| 1 | **14 role/group pairs were inert** (AttackingFB, WingBack, ShadowStriker, Mezzala, AdvancedPlaymaker, BallPlayingCB, InvertedFB, both keeper roles, DeepLyingForward, PressingForward, and the wide roles in a group the resolvers do not draw): the table had an entry, the resolvers never asked. A role that never fires leaves a match **bit-for-bit** unchanged, and 0 of 20,000 audited matches changed. The AI assigns them (about 0.4 AttackingFB, WingBack, ShadowStriker, BallPlayingKeeper, SweeperKeeper and InvertedFB a squad) | RUN | They act, through *support* (section 3) |
| 2 | **The roles that did fire were free gains**: every penalty in the table (AnchorMan attack 0.90, Poacher defence 0.85, DefensiveFB attack 0.93, DeepLyingPlaymaker attack 0.93) sat on a phase the holder's position is not drawn on, so none fired | RUN, READ | Documented, not changed (section 3.5): wiring them turned working roles into net costs |
| 3 | **The squad model's formation score does not merely fail to predict results, it points the wrong way**: with 192 clubs it correlates **−0.14 ± 0.03** with points across a club's own eight formations. P6's "slope is nil" (24 clubs) was underpowered. The cause is a level per shape: 4-3-3 is rated a point above 4-4-2 and earns **0.11 points a match less**; with each shape's level removed the squad-specific part is nil (−0.02 ± 0.03) | RUN | Section 4 |
| 4 | **The AI's own choice earns less than a random shape.** Taking the squad model's favourite earns **−0.054 ± 0.013** points a match against a random formation on worlds 1-12 and **−0.044 ± 0.011** on fresh worlds 13-24, where it sends **120 of 192** clubs to 4-3-3 | RUN | An engine-aligned evaluator was built and validated (4.4); **not adopted as the AI's rule** (4.5): ranking by it moves the league's goal rate from 3.18 to 3.65-3.81 |
| 5 | The engine's own `attack_rating` / `midfield_rating` / `defense_rating`, P6's "engine-aligned" comparator, **do not read the attributes the contests read** (`defense_rating` uses strength and the keeper's pace; the midfield contest uses dribbling, passing, vision, teamwork) | READ | `season-harness::match_study::lineup_features` reads what the contests read |
| 6 | The five-zone engine and M12 **agree on the extremes of formation price** (5-3-2 worst, 3-4-3 near the top) and **disagree on 4-3-3 and 4-5-1**; no reweighting of the numbers terms reproduces M12's prices (misfit 38.6 against 45.0 for pricing nothing) | RUN | None (section 5): not enough evidence to move a coefficient |
| 7 | M12 prices most tactical dials **6 to 8 times** more strongly on shots than the five-zone engine, and in three cases in the opposite direction (VeryLow line, High line, and the Possession style's possession) | RUN | None (section 5) |
| 8 | In M12 a play **style reaches the engine as mentality alone**: Possession does not play short and patient, High Press does not press | READ, RUN | Tried, measured, **reverted** (section 5.3) |

## 2. Baseline and commits

Baseline: P6, `78715a44`. Reproduced digit for digit by this tree with the role support weight at 0 (slope −0.0142 ± 0.0294, within-club
spread 0.713 and range 2.182 over 237 clubs), including AI-assigned roles on real worlds. P6 reference on six real worlds (6,840 matches
each, same seeds): **3.179 goals a match, 25.68 shots, 7.72 on target, 21.8 % draws, home points 1.561, home possession 51.78 %**.

| Commit | Change |
| --- | --- |
| `7b4f2981` | `feat(engine)`: roles the resolvers never consulted now act, through support (+ `p61_roles`, `p61_world_roles`) |
| `6588c23d` | `feat(ai)`: a club's formation is ranked by what the engine makes of its eleven (`formation_fit`). **Superseded by the commit below** |
| `af7c2b01` | `test(p61)`: formation-selection and dial studies, shared equal-player setup |
| `da76b5b7` | `fix(ai)`: scores the squad at full strength, margin 1.0. **Superseded by the commit below** |
| `d51e979c` | `fix(ai)`: the squad model ranks formations again; the engine fit is a shape-neutral measure carried on every score (goal-rate finding) |
| (the commit that adds this file) | `docs(p6.1)`: this document and the validation report |

## 3. Tactical roles

### 3.1 How a role reaches a contest

The five-zone resolvers draw players **by group** (a random defender in the attacking-third contest, a random midfielder in the midfield
contest, a random forward as the attacker and the shooter) and ask only that player's role about the phase the contest represents
(`consulted_phases`: a defender is asked about Defence, a midfielder about Midfield and Defence, a forward about Attack, a goalkeeper
about nothing). A table entry for any other phase never fires. That is the whole of the inertness: 14 pairs, no bug in any one of them.

### 3.2 The audit (RUN)

`engine/tests/p61_roles.rs::p61_role_audit`: every role, in every group that may hold it, against the same role-free side on the **same
seeds**, 70-rated 4-4-2s on both sides. "changed" is the share of matches whose result, shots or possession differ at all; **0 is inert,
not small**. 20,000 matches a row, the whole group holding the role (an upper bound), points a match gained ± standard error:

| Role (group) | P6: changed / gain | P6.1: changed | P6.1 gain ± se | goals for / against |
| --- | --- | --- | --- | --- |
| **AttackingFB** (D) | 0 % / 0 | 72.4 % | **+0.001 ± 0.008** | **+0.032 / +0.033** |
| WingBack (D) | 0 % / 0 | 68.3 % | +0.028 ± 0.008 | +0.051 / +0.010 |
| ShadowStriker (M) | 0 % / 0 | 81.2 % | +0.032 ± 0.009 | +0.057 / +0.012 |
| Mezzala (M) | 0 % / 0 | 52.4 % | +0.035 ± 0.007 | +0.042 / −0.009 |
| AdvancedPlaymaker (M) | 0 % / 0 | 60.1 % | +0.047 ± 0.008 | +0.058 / −0.011 |
| BallPlayingCB (D) | 0 % / 0 | 35.1 % | +0.013 ± 0.006 | +0.014 / −0.001 |
| InvertedFB (D) | 0 % / 0 | 40.3 % | +0.016 ± 0.006 | +0.016 / −0.003 |
| WideForward / InsideForward (as a midfielder) | 0 % / 0 | 52.4 / 60.1 % | +0.035 / +0.047 | +0.042 / +0.058 |
| InvertedWinger (as a forward) | 0 % / 0 | 29.5 % | +0.007 ± 0.005 | +0.009 / −0.001 |
| DeepLyingForward (F) | 0 % / 0 | 22.9 % | +0.005 ± 0.004 | +0.007 / −0.002 |
| SweeperKeeper / BallPlayingKeeper (GK) | 0 % / 0 | 8.8 / 12.3 % | +0.003 / +0.002 (se 0.003) | small |
| PressingForward (F) | 0 % / 0 | 6.1 % | +0.004 ± 0.002 | +0.004 / −0.002 |

The roles that already worked, for reference (P6 = P6.1 where no support applies): Stopper and DefensiveFB +0.029, CoverCB +0.018,
AnchorMan +0.033, BallWinner +0.025, DeepLyingPlaymaker +0.031, Carrilero +0.015, TargetMan +0.024, **Poacher +0.040**, CompleteForward
+0.032 (+0.036 with support), BoxToBox +0.019 → +0.046 and False9 +0.016 → +0.019 (the two that also have a bonus on a phase their
group is not drawn on). With **two** holders (the realistic case, 30,000 matches) every role is within ±0.03; with one, within ±0.02.

**Classification** (points a match, whole group holding the role, 20,000 matches). *Measurable*: a points effect of 3 or more standard errors
(Stopper, DefensiveFB, CoverCB, AnchorMan, BallWinner, DeepLyingPlaymaker, BoxToBox, Poacher, TargetMan, WideForward, InsideForward,
CompleteForward, Mezzala, AdvancedPlaymaker, WingBack, ShadowStriker), or, for AttackingFB, a goals effect of 4 or more with points netting
to zero. *Partially effective*: it acts but below 3 standard errors (BallPlayingCB +0.013, InvertedFB +0.016, Carrilero +0.015, False9
+0.019, InvertedWinger +0.007, DeepLyingForward +0.005, PressingForward +0.004, both keeper roles +0.002 / +0.003): a hundredth or two of a
point a match, under a point a season. *Inert*: none, where there were 14 pairs.

### 3.3 The change

`MatchConfig::role_support_weight` (`#[serde(default)]` = 0, shipped **1.0**). A holder outside the group a contest draws from **joins its
pool with one ticket**, so his share is `entry / (pool size + 1)`: what he would be worth if he were drawn like the rest. One derived
rule, not a fitted coefficient per role: a full back's attacking role counts in the attacking-third contest as one forward more, a ball-
playing centre back's in the midfield, a sweeper keeper's in the defence, a pressing forward's in the press. The product is capped at ±15 %
and is exactly 1.0 when nobody holds such a role, so **no random number is drawn differently**: a side with no such roles, and any match
played at weight 0, is bit-for-bit the P6 engine. The three roles designed with a cost (AttackingFB defence ×0.93, WingBack ×0.97,
ShadowStriker ×0.92, removed in `18b090de` because their bonus did not fire) pay it again, since their bonus now exists. The legacy
`engine::simulate` is untouched and still differs from the live engine, as in P6.

### 3.4 Trade-offs and dominance (RUN)

| | Result |
| --- | --- |
| AttackingFB is a real trade-off | Scores +0.032 and concedes +0.033; points +0.001 ± 0.008 against an equal side, **−0.008 as an underdog (6 points down), +0.013 as the favourite** (two holders, 30,000 matches, ±0.005): it pays when you have the ball |
| Nothing dominates | Largest gain of any role **+0.047** (AdvancedPlaymaker); Poacher +0.040, unchanged. P6's best dial stack was +0.08 to +0.11 |
| Stacking | Wing backs, advanced playmakers, mezzalas and a sweeper keeper **at once**, the whole group each: **+0.058 ± 0.019** points a match over a Standard side (4,000 paired matches; cap and dilution; test bound 0.2), inside P6's +0.08 to +0.11 for its best dial stack |
| On real squads | Six worlds, same seeds (6,840 matches each): goals a match **3.179 → 3.215** (+1.1 %), shots 25.68 → 25.94, on target 7.72 → 7.79, draws 21.8 → 21.4 %, home points 1.561 → 1.542 (this is also the shipped state: the AI's formation rule is unchanged, 4.5) |

### 3.5 What did not work, and what is left

1. **Crediting penalties as support failed.** The first design credited every table entry. AnchorMan's attack penalty (0.90), charged to the
   whole side's attack, outweighed his defensive bonus, which is paid only when he is the man drawn: AnchorMan went from +0.014 to
   **−0.025 ± 0.006** (two holders, 30,000 matches), DefensiveFB and DeepLyingPlaymaker also negative, in every match measured. A role
   that is slightly useful today became a trap. Support now credits a **bonus only**; a penalty is charged where its holder is drawn
   and nowhere else (test `a_role_that_already_worked_is_bit_for_bit_the_same_with_support`).
2. **Full weight per holder failed** (without the pool dilution, a holder's bonus counted in full while a drawn holder's counts 1/n of
   the time): the same net-negative roles, and Poacher −0.025. The per-ticket share fixed it by construction.
3. **The old roles' trade-offs remain missing.** AnchorMan, DefensiveFB, DeepLyingPlaymaker and Poacher still have their penalty on an
   unconsulted phase, so they are free upside (small: +0.015 to +0.040). Restoring them needs an opponent-dependent design (a defensive
   role should be worth more to the underdog), not another multiplier; it was not attempted.
4. **M12 consumes every role, but only as input presets** (positional freedom, forward runs, risk, pressing; the adapter says "M12 v1
   does not natively model these roles"). Whether each role does anything in M12 was not measured: at a second a match a 0.02 effect
   needs about 10,000 matches.

## 4. Formation selection

### 4.1 Why the squad model's score fails (RUN)

The squad model scores a formation by the mean positional fit of the eleven slots. 192 clubs in 12 independent worlds, every formation
played against one fixed panel of six opponents (96 matches a cell), the score regressed within club across the eight shapes:

| | |
| --- | --- |
| Fitted slope of goal difference on the score | **−0.054** a rating point (negative) |
| Within-club correlation with points | **−0.141 ± 0.028** |
| Mean score by shape (rating points) and mean points a match | 4-4-2 67.58 / 1.564, **4-3-3 68.57 / 1.454**, 3-5-2 67.30 / 1.607, 4-5-1 67.66 / 1.533, 4-2-3-1 67.66 / 1.549, 3-4-3 66.75 / 1.555, 5-3-2 67.74 / 1.462, 4-1-4-1 67.72 / 1.496 |
| With each shape's cross-club mean removed from both | **−0.020 ± 0.032**: nothing is left |
| Mean stored overall of the eleven, same test | +0.052 ± 0.030 (shape kept), +0.036 ± 0.031 (removed) |

1. **The variation is a level per shape, and the level points the wrong way.** Attacking slots rate higher, so 4-3-3 stands a point above
   4-4-2 and earns 0.11 less. This is what made P6's within-club standard deviation 0.71 points and its slope nil.
2. **Wrong attributes are a minor part.** At most 18 % of any slot's rating weight is on attributes the contests never read (strength,
   leadership; stamina matters only through fatigue): centre back 14 %, wing back 18 %, full back 16 %, striker 8 %, attacking midfielder 0 %.
3. **The engine reads groups, not slots.** A player counts in the group of the slot he stands in with his own attributes; there is no
   notion of a player out of position (`to_engine_player`). The score prices fit to a slot, which the engine never asks.

### 4.2 Candidates, scored out of sample (RUN)

`season-harness/tests/p61_formation_selection.rs`: leave one world out (each club is scored by a rule fitted on the other 11 worlds), 192
clubs, ordinary least squares on within-club deviations. "Gain" is the points a match the rule's pick earns over a club's mean across its
eight shapes (a random shape earns 0); the ceiling is the split-half correlation of two independent halves of a club's matches, +0.405.

| Candidate | Correlation (se) | Gain over a random shape | Best − worst shape |
| --- | --- | --- | --- |
| Squad model's score (today's) | +0.141 (0.028) *(anti: fitted slope negative)* | +0.045 (0.013) *(by choosing its **least**)* | +0.115 |
| Mean stored overall of the eleven | +0.052 (0.030) | +0.009 (0.013) | +0.011 |
| The shape alone (7 dummies) | +0.267 (0.026) | +0.079 (0.012) | +0.137 |
| Own contest means (8) | +0.486 (0.028) | +0.123 (0.012) | +0.267 |
| **Back line + finishing only (2)** | +0.478 (0.028) | +0.115 (0.012) | +0.253 |
| **Shape + back line + finishing (9)** | **+0.534 (0.025)** | **+0.131 (0.012)** | **+0.288** |
| Contest ratios against the panel, with the engine's numbers terms (8) | +0.518 (0.026) | +0.119 (0.012) | +0.275 |
| Shape + own contest means (15) | +0.528 (0.025) | +0.124 (0.012) | +0.282 |
| Suitability to the shape: contest means less what a typical squad reaches in that shape (8) | +0.448 (0.030) | +0.104 (0.012) | +0.239 |
| Contest ratios + squad score (9) | +0.518 (0.025) | +0.117 (0.012) | +0.273 |
| Suitability + squad score (9) | +0.469 (0.029) | +0.114 (0.012) | +0.257 |

**The squad score adds nothing to any engine-aligned rule** (0.518 → 0.518, 0.448 → 0.469 for the suitability rule; its fitted weight is −0.017, −0.054 and −0.007 in the three rules that include it, and negative every time). Two means carry
almost all of it: the back line's hold (defending, tackling, positioning, aerial: the defender in the attacking-third contest) and the
forwards' finishing (shooting, composure, decisions: the shot). 15 parameters do no better than 9, which do no better than the
2 + the shape's price.

### 4.3 Opponent formation (RUN, equal players)

The 8 × 8 five-zone matrix at 20,000 matches a cell (`p6_equal_players_shape_matrix`) is **additive to within noise**: fitted as a row
effect plus a column effect, the rms interaction is 0.008 points a game and the largest 0.022 (5-3-2 against a 4-4-2 and against the
five-midfielder shapes), against a cell standard error of 0.009. A shape's price against a 4-4-2 and against the field differ by under
0.02 for every shape. The **opponent's shape is a second-order term**, so it is not in the score: the contests read counts through the numbers terms only, and a rule built on the
panel's group means (the "contest ratios" row) does no better than the opponent-free one.

### 4.4 The evaluator: `ofm_core::formation_fit::engine_fit` (RUN)

The eleven the club would field (`first_choice_eleven`, the whole squad at full strength), each player counted in the group of the slot he
stands in with his own attributes. Two means: the back line's hold and the forwards' finishing. The score is how far each is **above what a
typical squad reaches in the same shape** (eight pairs of constants, the mean over 192 clubs), weighted 0.026 and 0.036 goals of goal
difference a match a point, and converted to the squad model's own unit, a mean rating point of the eleven, at **0.136 goals a match**
(measured: +1 to +5 uniform rating points gave 0.134 to 0.137 each, `p61_points_per_rating_point`). It carries **no price for a shape
itself** (test `a_typical_squad_is_worth_the_same_in_every_shape`, mutation-checked): the first version had one and is why 4.5 happened.

Out of sample, leave one world out (192 clubs, 12 worlds): correlation **+0.452 ± 0.030**, gain **+0.113 ± 0.013** points a match over a
random shape (the nine-parameter version with shape prices: +0.534 / +0.131). **Frozen, on worlds it was never fitted on**, the figures are
lower and I report these:

| Frozen rule, 192 clubs / 48 clubs | Correlation | Gain over a random shape | Best − worst |
| --- | --- | --- | --- |
| Fresh gate worlds 13-24: the evaluator | **+0.182 ± 0.031** | **+0.039 ± 0.013** | +0.089 |
| Fresh gate worlds 13-24: the squad model at face value (what the AI does) | −0.148 ± 0.025 | **−0.044 ± 0.011** | −0.089 |
| Shipped 440-club world, 3 worlds: the evaluator | +0.176 ± 0.065 | +0.074 ± 0.027 | +0.156 |
| Shipped 440-club world: the squad model at face value | −0.047 ± 0.056 | −0.032 ± 0.022 | −0.024 |

On the fresh worlds the squad model sends 120 of 192 clubs to 4-3-3; the evaluator spreads them (4-1-4-1 71, 3-4-3 44, 3-5-2 26, 4-3-3 20,
4-2-3-1 12, 5-3-2 11, 4-4-2 8). The nine-parameter rule with shape prices, frozen the same way, earned +0.073 ± 0.012 (correlation +0.290).
The ceiling of any rule on these worlds is a split-half correlation of +0.34. **The evaluator is a real but modest predictor; the squad
model is a reliably negative one.** It was not tested on the imported FM24 world.

### 4.5 Adopting it as the AI's rule: what happened, and why it was not adopted (RUN)

The one-shot ranking is not the AI: the weekly review has a margin, an inertia and a taste, and clubs that all rank the same way change
what the league looks like. Three gate worlds, ten seasons, invariants on, the same seeds:

| Rule in the weekly review | Formations in use | Top shape's share | **Changes a season** (80 clubs) |
| --- | --- | --- | --- |
| P6 (squad model, margin 0.25) | 5.9 | 46.7 % | **23** |
| Engine fit with shape prices, margin 0.25, players who can play today | 6.5 | 55.4 % | **110** |
| + the whole squad at full strength | 6.3 | 55.8 % | 67 |
| + margin 0.5 / 1.0 / 1.5 | 6.4 / 6.2 / 6.1 | 55.0 / 57.4 / 56.0 % | 54 / 34 / 26 |

1. **Scored on who can play today, a shape followed the treatment room** (110 → 67 on counting the whole squad; test
   `a_squads_shape_does_not_move_with_this_weeks_injuries`). 2. **The margin was calibrated to a different score**: 0.25 suited a score that was
   a level per shape; the engine's fit follows a squad that is always changing. The engine charges nothing for changing shape, so the margin
   is pure realism; the sweep gave 54 / 34 / 26. 3. The two rules are equally steady with formations held (0.8 % against 1.4 % of weeks the
   best shape changes, `p61_formation_stability`): the extra churn was a response, not noise. These were fixed, and then the **league's
   goals** were measured, which is what decided it.

**The goal rate.** Six worlds, 6,840 matches each, the same seeds, warmed 60 days with the reviews running:

| AI's formation rule | Goals a match | On target | Draws | Clubs in a back three | Clubs in a one-forward shape |
| --- | --- | --- | --- | --- | --- |
| **P6 (squad model): the shipped state** | **3.215** | 7.79 | 21.4 % | 4 of 120 (3-5-2) | 15 |
| Engine fit with shape prices | **3.649** | 8.19 | 18.9 % | 60 (3-5-2 47, 3-4-3 13) | 35 |
| Engine fit, shape-neutral | **3.810** | 8.44 | 19.6 % | 23 | 51 |

Roles are on in all three; the P6 engine's own figure with roles off is 3.179. Shots a match are flat (25.6 to 25.9) in every row: the extra
goals are **shot quality**, not volume. Every club forced into the same shape gives the league's goal rate by shape:

| All 120 league clubs in | 3-4-3 | 4-3-3 | 3-5-2 | 4-1-4-1 | 4-5-1 | 4-2-3-1 | 5-3-2 | 4-4-2 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Goals a match | 3.13 | 3.19 | 3.35 | 3.35 | 3.43 | 3.45 | 3.45 | 3.51 |

- **P6's 3.18 was the 4-3-3 world.** The squad model's mass move to 4-3-3 (64 % of clubs) kept goals at the bottom of a range of
  3.1 to 3.5, because three forwards dilute the finishing mean.
- **The five-zone engine reads a group by its mean**, so a squad's best players in a thin group (one striker, three centre backs) beat the same
  players in a full one. A rule that picks each club's best shape lifts every club's finishing and back line together, and the league's goals with
  them: 3.65 to 3.81, **above every uniform world**. My first explanation, a herd into back threes, was wrong: the shape-neutral rule has no
  such herd and scores more.
- This is a property of the engine (P6 flattened its dials and priced numbers; the concentration of quality in a thin group is still free).
  Fixing it means changing how a group's contribution is read, which is not a coefficient and not this phase's to do.

**Decision.** By the rule this phase set itself (replace the existing model only on a credible improvement), the improvement in *predicting* is
credible and the improvement in *the game* is not: adopting it moves the league's goals by 15 to 20 % and its draws by 2 points, outside what P6
calibrated and what the game's realism gates are written for. **The AI's formation rule is unchanged from P6** (squad model, margin 0.25), the
career tables above reproduce the P6 behaviour **bit for bit** (5.9 / 46.7 % / 23 on every seed-season), and the evaluator is carried on every
`FormationScore` as data. It is what I2 should use once the engine reads a thin group differently.

## 5. Five-zone engine against M12

### 5.1 Formation price at equal players (RUN)

Five-zone: 20,000 matches a cell. M12: P6's runs (500 matches a shape, se about 0.06; **not rerun**: a second a match).

| Shape, points a match against a 4-4-2 | Five-zone | M12 | Agree? |
| --- | --- | --- | --- |
| 5-3-2 | −0.057 | −0.162 | yes |
| 4-1-4-1 | −0.026 | −0.078 | yes |
| 4-2-3-1 | −0.026 | −0.030 | yes |
| 3-4-3 | +0.009 | +0.226 | yes (five-zone weak) |
| 3-5-2 | +0.027 | −0.024 | within M12's noise |
| **4-3-3** | −0.046 | **+0.120** | **no** |
| **4-5-1** | −0.026 | **+0.098** | **no** |

4-5-1, 4-2-3-1 and 4-1-4-1 are one shape to four groups (three in M12, spread 0.18). **No reweighting closes it:** P6's grid fit re-run on
this engine gives best pair 0.2 / 0.2 at misfit 38.6 against 45.0 for pricing nothing (0.2 / 0.4: 42.0; 0.0 / 0.2: 44.7; the rest worse).
M12's own standard error (0.06) makes a 6-unit difference between 0.2 / 0.2 and the shipped 0.3 / 0.3 unresolvable. **No coefficient was changed.**

### 5.2 Tactical dials (RUN)

`p61_dials_both_engines`: two equal-player clubs, one setting moved against a neutral 4-4-2, home and away, the same seeds in both engines
(five-zone 3,000 matches; M12 400, **points se 0.09, shots se 0.24**). Change from the subject's own neutral baseline:

| Setting | Five-zone: points / shots / possession % | M12: points / shots / possession % | Direction |
| --- | --- | --- | --- |
| Defensive | +0.010 / −0.21 / −0.0 | −0.152 / **−1.73** / +0.8 | shots agree |
| Attacking | −0.021 / +0.30 / +0.1 | −0.125 / **−0.73** / **−2.7** | shots **disagree** |
| Possession | +0.088 / +0.38 / **+2.3** | −0.058 / −0.34 / −0.9 | possession **disagrees** |
| Counter | +0.035 / +0.07 / −1.7 | +0.115 / +0.03 / +0.3 | not resolvable |
| HighPress | −0.029 / −0.08 / −0.0 | −0.013 / **−0.66** / −1.7 | shots agree |
| VeryLow line | +0.006 / −0.16 / −1.5 | **−0.388** / **+2.56** / **+8.6** | points and shots **disagree** |
| High line | +0.015 / +0.06 / +0.65 | −0.092 / **−0.98** / **−3.1** | **disagree** |
| Passive press | −0.005 / +0.25 / −0.3 | −0.102 / **+1.46** / +1.9 | agree |
| Aggressive press | +0.013 / −0.25 / +0.5 | **−0.237** / **−1.88** / **−2.5** | shots agree, points **disagree** |
| Narrow | +0.049 / +0.14 / +0.1 | −0.058 / +0.15 / −0.1 | shots agree |
| Wide | −0.038 / −0.13 / −0.0 | −0.028 / −0.03 / +0.2 | agree |
| Patient tempo | +0.002 / −0.16 / +0.3 | −0.065 / **−0.51** / −1.0 | shots agree |

1. M12 prices most dials **6 to 8 times** more strongly on shots (Defensive −1.73 against −0.21, Aggressive −1.88 against −0.25, Passive
   +1.46 against +0.25), and the very low line in the opposite direction. P6 deliberately flattened the five-zone dials to remove free gains.
2. The largest disagreement is the **very low line**: **−0.39 points a match in M12 (4.2 standard errors; about 15 league points a
   season)** against +0.006 in the five-zone engine at equal players (−0.053 on real squads, P6). M12 also gives it +8.6 points of possession
   and 2.6 more shots.
3. **Not changed.** Moving a five-zone dial to M12's size would reintroduce the free gains and costs P6 removed in the engine careers are played
   with; one pair of clubs and 400 matches resolves a 0.2 effect at two standard errors; and M12's mechanism (a deep block) is geometry no
   multiplier reproduces. A credible alignment needs at least 2,000 M12 matches over several club pairs.

### 5.3 A play style reaches M12 as mentality alone (READ, RUN): tried and reverted

`spatial_adapter::tactics` sets M12's `mentality` from the style (Defensive 30, Counter 45, Possession 55, HighPress 60, Attacking 70) and
nothing else: the style never touches tempo, directness or pressing. So a **Possession style is not short and patient, and a High Press style
does not press**, in the match a person watches. I added style offsets of one dial step (Possession tempo and directness −15, HighPress
pressing +20; Counter left alone: its M12 difference, +0.3 against −1.7 points of possession, was unresolvable) with a test red before, and
measured the same seeds against M12 (400 matches):

| | Before | After | Five-zone |
| --- | --- | --- | --- |
| Possession: points / possession % | −0.058 / −0.89 | −0.010 / −0.22 | +0.088 / **+2.29** |
| HighPress: points / shots / possession % | −0.013 / −0.66 / −1.66 | **−0.128** / −1.09 / −2.67 | −0.029 / −0.08 / −0.01 |

Possession's possession moved +0.67, within noise, and stayed negative. HighPress now presses and pays for it in M12 (−0.128 points), where the
five-zone engine says about −0.03: a *larger* gap. By the rule this phase set itself (change a coefficient only on evidence), **the change was
reverted**; the finding and the unresolved question stay. It is the first thing to try again with 2,000+ M12 matches.

### 5.4 What is consistent and what is not

Consistent: the ends of the formation price list (5-3-2 worst, 3-4-3 high), the direction of most shot effects (Defensive, HighPress, Passive,
Aggressive, Patient). Not consistent: 4-3-3 and 4-5-1, the Possession style's possession, the very low line, the high line, the size of every
dial. These are differences between a four-group engine and a geometric one, and **the engines still price tactics differently**: this phase
measured it dial by dial and changed nothing, rather than moving a coefficient the evidence cannot support.

## 6. Limits and what is left

1. **The AI still ranks its formations by a score that points the wrong way** and sends most clubs to 4-3-3 (120 of 192 on fresh worlds; 4-3-3
   earns 0.11 points a match less than 4-4-2). The better evaluator is built and measured but not used, because the engine's reading of a
   group by its mean makes a good rule move the league's goals by 15 to 20 % (4.5). **The next lever is in the engine**: read a thin group of a
   squad's best players as less than the best of them (for example by weighting the shooter by the group's depth), then adopt the evaluator and
   re-sweep the margin (the sweep in 4.5 is the starting point: 1.0 gave 0.43 changes a club-season).
2. The evaluator is a **modest** predictor out of sample (+0.18, gain +0.04 on fresh gate worlds; +0.18, +0.07 on the shipped world), well below
   its cross-validated figures (+0.45, +0.11), and was **not tested on the imported FM24 world**. The league's goal rate with a different AI is a
   property of the five-zone engine and was not tested under M12.
3. **The AI's selection of the eleven** (`first_choice_eleven`) still maximises positional fit, the number shown above to be a poor guide, not the
   contests' attributes. It was not touched.
4. Roles: the old roles' costs are still unwired (3.5); PressingForward, the keeper roles and four others act at a hundredth of a point or below;
   M12's reading of roles was not measured. Role support adds 1.1 % to the league's goals.
5. M12 was not run on formations again, and its dial effects rest on one club pair at 400 matches (5.2). Not one of the five-zone / M12 differences
   in 5.1 to 5.3 was closed.
6. The match-day formation adjustment (`matchday_formation`) stays disabled (P6); the evaluator has not been tried there.
7. **The GUI was not played.** No desktop control exists in this session; nothing here is claimed from the window. What a person would see:
   AttackingFB / WingBack / ShadowStriker (and the roles in 3.2) now doing something in a quick-sim match; AI clubs' formations are as in P6.
8. The project's reviewer agents (`ofm-architecture-reviewer`, `ofm-dedup-reviewer`, `ofm-test-reviewer`) were **not run**: the phase was not opened
   as a pull request. A reviewer pass is owed before one is.

## 7. For I2 (compatibility requirements)

1. **No saved field changed.** `MatchConfig::role_support_weight` is `#[serde(default)]` (an older configuration reads 0); `FormationScore` is not
   persisted; no SQL, no migration. **Determinism holds:** two identical runs of the formation study agree to every digit; at weight 0 the P6
   baseline reproduces digit for digit; and the AI's formation behaviour is **unchanged from P6** (three ten-season careers give the same tables
   to the cell, 23 changes a season, invariants clean).
2. **What I2 will see differ from P6** is the match engine only: roles now act (3.2), which adds **+1.1 % goals** (3.179 → 3.215) and nothing else
   measurable in a league; long-run results are not comparable to P6's seed for seed, as in P6.
3. `ofm_core::ai_formation::FormationScore` has a new public field `fit` (**anything constructing one by hand must add it**); `total()` is
   `squad + taste` as before and `BASE_MARGIN` is 0.25 as before. `ofm_core::formation_fit::engine_fit` is crate-private.
4. `engine::MatchConfig` has a new public field `role_support_weight` (default 1.0); anything building it field by field (not
   `..Default::default()`) must add it. Nothing in the tree does.
5. A role set by the player now has an effect where it had none: the UI's role descriptions are unchanged and **no string was added or touched**
   (no locale file was modified).
6. Anything that reads the engine's `TeamData::{attack,midfield,defense}_rating` as "what the contests read" is wrong (finding 5); use the attribute
   means in `season-harness::match_study::lineup_features`.
7. **Do not adopt `fit` as a ranking before the engine change in 6.1**: it moves the league's goals by 15 to 20 % (4.5).
8. The M12 adapter is **unchanged** (section 5.3 was reverted), as are the registry, importer, recruitment and financial files, so **nothing here
   touches D7B3's files**.
