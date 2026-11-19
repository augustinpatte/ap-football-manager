# APFM I5: tactical intelligence

Branch `deepseek/i5-tactics`, from `f31dfa2669f68e0025e3668c8553dd4eb116db2a` (I4). **Not pushed, not
merged.** Measurements, commands and limits are in [`APFM_I5_VALIDATION.md`](APFM_I5_VALIDATION.md).

Evidence labels: **RUN** (executed this phase), **READ** (read in source). Standard errors are over
clubs or worlds as stated.

## 1. What was wrong, and what I5 does about it

| # | Finding | Where it bit | Change |
| --- | --- | --- | --- |
| 1 | **The AI picked formations by a number that does not predict results.** The squad model's formation score, taken at face value, correlates **−0.061** with a club's own points across its eight shapes and picking its favourite earns **−0.025 points a match**, less than a random shape | the weekly review, and every AI club's identity | the review ranks by what the engine makes of the eleven, plus taste (section 3) |
| 2 | **The better evaluator was designed, measured and switched off** because it lifted the league's goals a match by 15–20 % | `formation_fit` | the *cause* is fixed: the engine read a thin group by the plain mean of the men in it, so a squad's best players concentrated into a thin group were worth the best of them (section 2) |
| 3 | **4-3-3 was the squad model's favourite for two clubs in three** (129 of 192, and 53 % of a career's club-seasons), not because it was better but because the squad model rates it higher for reasons the engine never sees | every AI league | ranking by the engine's reading puts 37 % of 192 clubs in 4-4-2, 20 % in 3-5-2, 15 % in 4-1-4-1 and the rest across the other five, with 4-3-3 down to 6 % (RUN) |
| 4 | **A club's plan was a plan for nobody**: it was built from its own squad and its manager's identity, and never looked at the side in front of it | every kick-off | a pre-match response, at most two dials, from a fixed table of answers the engine charges for (section 4) |
| 5 | **The match-day manager had no reaction to a sending-off** and a side that went a man down kept pressing high | minutes 60–90 | a man down and not behind absorbs; a man up and not ahead pushes (section 4) |
| 6 | **Five-zone and M12 were described as two readings of one plan but never set side by side** | any claim that a dial means the same thing in both | the nine dials, the play style and the four phase groups are mapped one to one into M12's own scales; the comparison and the unmodifiable dependency are in section 5 |

Nothing here is a quota. Every change is a rule the engine can price: a shape is better when the
engine plays it better, a dial is turned when the engine charges for turning it, and a formation is
abandoned when the engine's reading beats it by more than changing costs.

## 2. How the engine reads a thin group (the root cause)

The five-zone resolvers read a group by the mean of the players standing in it: a random forward
shoots, a random defender faces him, the men behind a shot are the mean of the defenders. That mean
was taken over the *deployed* group, so a squad's best players concentrated into a thin group were
worth the best of them: one 90-rated finisher alone up front shot like a 90-rated finisher every
time, while the same man in a front three shared the shots with two 60s. The same held for three
international centre backs behind a back three.

That is a free lunch, and every rule that read the engine's group means bought it. It is why P6.1's
evaluator — which scored a squad by *how far above a typical squad in that shape* it read — sent
half the league into a back three or a one-man front line and lifted the league's goals a match from
3.18 to 3.65–3.81. The evaluator was not wrong about the engine; the engine was pricing thinness as
a strength.

**The rule** (`engine::group_depth`, one implementation for both callers):

> A group short of a full one is read as if the slots it leaves empty were filled by an average
> outfield player of the same side, up to a full group: four defenders, four midfielders, two
> forwards — the shape the engine's own numbers terms are written against. A full group is
> unaffected.

- **Level-relative, not fixed.** The missing slots are filled at the side's own standard (the mean of
  the outfield eleven on the same attribute read), not at 50, so a world whose players are all better
  or worse moves every reading alike and changes no choice.
- **One weight, one place.** `MatchConfig::group_depth_weight`, `#[serde(default)]`, shipped `1.0`.
  A configuration written before the field reads `0`, which is the plain mean and bit-for-bit the old
  engine.
- **Where it applies.** Every contest that reads a group: the midfield contest and the possession
  contest (both sides), the attacking third (both sides), and the shot (the shooter and the defenders
  screening him). The numbers edge is untouched: how many players stand in a group is still a fact
  about the shape, and width is still what a back three buys.

**The trade-off.** A back three now pays for its fourth defender at the side's own standard, and a
lone forward pays for the second half of his front two. That is a price, not a prohibition: three
centre backs who are much better than the club's fourth defender are still worth a back three, and a
lone striker is still worth a 4-5-1 when the midfield he adds is worth more than the forward he loses.
It also removes the *reason* the evaluator inflated goals, so a rule may read it.

## 3. The formation evaluator

`ofm_core::formation_fit` is the engine's own reading of the eleven a club would field in a shape,
and nothing else:

```text
engine_fit = (0.022 × back_line + 0.010 × midfield + 0.027 × finishing) / 0.136
```

| Group | Read as | Goals a match per point |
| --- | --- | --- |
| back line | defenders' defending, tackling, positioning, aerial, at their depth | 0.022 |
| midfield | midfielders' dribbling, passing, vision, teamwork, at their depth | 0.010 |
| finishing | forwards' shooting, composure, decisions, at their depth | 0.027 |

- The **weights** are fitted on 192 clubs in 12 worlds, every club in all eight shapes, out of sample
  by world (`season-harness/tests/p61_formation_selection`). They are the goals of goal difference a
  match a point of each group was worth in the league the study played. A repeat full run reads
  `+0.021`, `+0.014` and `+0.024`: the shipped three are the first fit, and all three are within the
  study's noise of the repeat. The rule is measured frozen, not refitted, in the table below.
- The **divisor** (0.136 goals a match per rating point of a whole side) is a scale, not a
  measurement: it puts the score in the unit the rest of the game scores formations in, so the margin
  a change must clear and a manager's taste keep the meaning P6.1 gave them.
- The **depth** of each group is the engine's rule, applied by `engine::group_depth` itself. The
  evaluator does not have a second copy of it; it maps the domain's group to the engine's constant and
  calls the one function.
- The **eleven** is the club's selection with nobody tired (`first_choice_eleven`): a shape is a
  decision about the structure of a squad, and scored on who can play this week it followed the
  treatment room.

**It is not normalised by a typical squad's level in the shape.** P6.1's form subtracted what a
typical squad reaches in the same shape, so that a shape itself carried no price. That was the right
rule for the engine it measured and the wrong one for a rule: it discards the engine's own, real
price of a shape. With the depth-aware reading in place the level of a shape *is* the engine's
reading of it, so it is read. The study sets the two forms side by side (RUN):

| Out-of-sample, a club's own eight shapes | Correlation with points | Gain over a random shape |
| --- | --- | --- |
| squad model's score, as the game played it (no refit) | −0.061 (0.029) | −0.025 (0.013) |
| a shape's own average | +0.231 (0.026) | +0.072 (0.012) |
| I5: depth-aware reading, three groups (refit) | +0.210 (0.033) | +0.072 (0.013) |
| **I5: the shipped rule (frozen weights)** | **+0.210 (0.033)** | **+0.075 (0.013)** |
| I5: suitability form, three groups | +0.172 (0.034) | +0.051 (0.013) |
| split-half ceiling | +0.318 (0.026) | — |

**What is not in the score, and why.** The score is `fit + taste`, where taste is the club's
philosophy and its manager's identity (`ai_formation`, unchanged since P6.1). The squad model's score
is still carried on every `FormationScore` as `squad`, and decides nothing: it is the number the
market and the squad screens read, and the studies read it. **Cover** — how many of a shape's slots a
club has no natural player for — was built, measured and left out: alone it is degenerate (it sends
191 of 192 clubs to 4-1-4-1), and added to the fit it changes no choice and earns nothing (RUN,
section 2 of the validation). **No randomness, no quota, no rotation** decides a shape.

**The change margin** is what stops a club oscillating: the best shape must beat the current one by
`margin`, which a new manager's mandate and a worrying run reduce and a vindicating run raises
(unchanged since P6.1). The margin is inertia, not calibration, and it is set from the measured
distribution of the gap between a club's best and current shape.

## 4. Match-day intelligence

### 4.1 Before kick-off: answering the opponent

`ofm_core::matchday_tactics::opponent_response` is a pure function of the two plans and the
opponent's play style, and it names at most two dials (`MAX_MATCHDAY_MOVES`), only for AI sides:

| The opponent | The answer |
| --- | --- |
| a deep block (line very low or low) | a higher line, and width to pull the block apart |
| a high line | break fast, and play over it rather than through it |
| an aggressive press | go long, so the press is played past rather than through |
| a wide shape | defend compact, and close the middle it left |
| a narrow shape | attack wide, where nobody is standing |
| an attacking side | drop the press, and make it pass through a shape |

Every answer has a price the engine charges: the high line leaves space behind it, going long gives
the ball away more often, fast breaks concede control of the ball, and a compact block concedes the
flanks. The response
never names a dial that is already where the answer wants it (a dial left where it was is not an
instruction), never exceeds the engine's own ration of under-priced dials
(`engine::ai::MAX_UNDER_PRICED_DIALS`), and is applied to the live match's copy of the side, so a
club's saved plan is the same after the match as before it. Both dugouts read each other **before**
either answer is applied: neither side answers a plan the other just changed.

There is no "play faster" answer available, and that is a limit of the engine, not of the rule: the
five-zone `Tempo` has two settings and `Direct` is both the faster one and the default, so a
match-day plan can only ask a side to be patient, which is the opposite of squeezing a block. The
answering dial for a block is therefore width, which the engine prices.

### 4.2 During the match

The engine's AI (`engine::ai`) already reacted to a goal, a red card and to fatigue; I5 adds the
sending-off to the play-style decision (RUN, three new tests in `engine/tests/ai_decision_tests.rs`):

- a man down from the hour and not behind: absorb (`Defensive`), rather than press high with ten;
- a man up from the hour and not ahead: push the advantage (`Attacking`);
- behind and late: unchanged from P6.1 — the side chasing gets its most attacking shape and style;
- ahead and late: unchanged — hold the shape and the ball.

Substitutions, the bench, and the red-card forced substitution are unchanged: P6.1 measured them and
I5 did not need to move them. Section 6 of the validation measures what a manager actually does in a
match with them all on.

## 5. Five-zone and M12 read the same plan

M12 (`apfm-match-lab`, pinned rev `28f34b10d4fe7cadacc0030acaaa0cd1aa9fdf5f`) is an external
repository, vendored through `ofm_core::spatial_adapter`. I5 cannot change it and does not.

**What is shared.** Both engines are handed the same `PlayStyle` and the same
`TacticsPhaseSettings`. `spatial_adapter::tactics` maps every one of the nine dials, the play style,
and the four phase groups onto M12's own 0–100 scales:

| Ofm instruction | M12 field | M12 value |
| --- | --- | --- |
| play style | `mentality` | 30 Defensive … 70 Attacking |
| pressing intensity | `pressing` | 25 / 55 / 85, plus counter-press ±8 and marking 0…6 |
| defensive line | `line_height` | 10 / 35 / 60 / 85 |
| width | `width` | 25 / 50 / 75, plus shape ±8 |
| tempo | `tempo` | 30 Patient / 70 Direct, plus break speed ±10 |
| build-up style | `directness` | 25 / 50 / 75, plus break speed ±10 |
| marking style, counter-press duration, defensive shape, break speed | folded into pressing / width / tempo / directness | as above |

**What differs, and why it is allowed to.** The instruction means the same thing in both engines — a
high line is a higher line in both, an aggressive press is a more aggressive press in both, and width
is width in both — but the *effects* do not all point the same way. The five-zone engine prices a
dial through its own contests (a multiplier on a rating); M12 prices it in metres and seconds. On
shots, seven of the twelve settings the paired study moves have the same sign in both engines; on the
high line and the very low line they diverge (section 7 of the validation), and on the dials it
resolves M12 prices the effect several times more strongly (a defensive setting is −2.05 shots
against the five-zone engine's −0.14; an aggressive press −1.63 against −0.23). This is the
pre-existing divergence P6.1 documented; I5 did not rescale a dial to paper over it, and no claim in
this document rests on the two engines agreeing on a magnitude.

**What I5 did not do.** No dial was added, renamed or re-scaled to make the two agree: the shared
vocabulary is the nine dials the game already exposes, and a change to M12's scales would be a change
to a pinned external dependency. The one thing I5 adds at the seam is the pre-match response, and it
is a sequence of ordinary `ChangeTacticalDial` commands on the live match, which M12 receives like
any other dial change.

## 6. Frontend

The nine dials are already exposed (`src/components/tactics/PhaseBlueprintPanel.tsx`, verified READ):
build-up style, width, tempo, defensive line, pressing intensity, defensive shape, marking style,
counter-press duration, break speed. I5 adds no instruction a player cannot set, so no production
frontend change is required and none was made. Every new string a player could read would be a
translation key in all locales; there are none.

## 7. Scope and seams

Not touched: staff hiring, delegation and training (I9), club finances, scouting, youth, I4A
recruitment, I4B competition lifecycle, the locale files, M12, the registry and importer. Shared
files touched, and why:

| File | Change | Why |
| --- | --- | --- |
| `engine/src/types.rs` | `MatchConfig::group_depth_weight` (`#[serde(default)]`) | the one weight of the new engine rule; a configuration written before the field reads 0 |
| `engine/src/shared.rs` | `defensive_quality` takes the weight | one depth-aware reading, both callers |
| `engine/src/live_match/{helpers,zone_resolution,simulation}.rs` | depth factors on the four group contests | the rule where the contests are |
| `engine/src/ai.rs` | red-card branches in `target_play_style` | match-day reaction |
| `ofm_core/src/ai_formation.rs` | the review ranks by `fit + taste` | the decision |
| `ofm_core/src/formation_fit.rs` | rewritten | the evaluator |
| `ofm_core/src/matchday_tactics.rs` | new | the pre-match response |
| `ofm_core/src/live_match_manager.rs` | applies the response before kick-off | the seam |

Saves stay compatible: no persisted type changed. `MatchConfig` is built per match from its
`Default` (weight 1.0), so a loaded career plays the new rule; the new field is `#[serde(default)]`,
so a serialized configuration written before it reads 0 and plays the old one. `FormationScore` is
not persisted, and no SQL or migration changed. No DB table, no repository, no migration count.
