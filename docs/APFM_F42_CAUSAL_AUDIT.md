# APFM F4.2 causal audit: from a ball recovery to a shot

Scope: the M12 football layer (`apfm-match-lab`, branch `f4.2/football-intelligence`, base `4057c67`), as
OFM uses it through `FootballConfig::full_match_m12()`. Everything here was read from the code and, where a
claim is about behaviour, checked with a controlled situation (`tests/f42_diagnostics.rs`, §3). File paths
are in `src/sim/football/` unless stated.

## 1. One possession, traced

```text
ball recovered ──► establish()            runtime.rs      owner set; possession change; transition window
      │                                                   (4 s, TeamPhase::TransitionAttack / Defence)
      ▼
 settle (250–600 ms by `decisions`) ──►  decide_ext()     decision.rs   re-evaluated every 250 ms
      │                                      │
      │            ┌─────────────────────────┼───────────────────────────┐
      ▼            ▼                         ▼                           ▼
 generate_options()  best_carry()        evaluate_shot()            hold utility
 passing.rs          decision.rs         shooting.rs                (pressure, time held)
      │
      ▼
 kick() ─► ball in flight ─► receiver: Task::Receive(aim) ; defenders: assign_flight / intercept
      │
 meanwhile, every 500 ms (attack) / 400 ms (defence), on the snapshot:
   plan_attack()  offball.rs   runs, support, width, depth     ──► Order{point, factor}
   plan_defence() defend.rs    press, contain, cut lane, mark, [shape_pass]
      │
      ▼
 maintain_owner(): a defender within reach of the ball, after `duel_grace_ms`, rolls a duel
      │            p = clamp(0.35 + 0.8·(defence − attack), 0.05, 0.75)      touch.rs
      ▼
 pass completes / is intercepted / duel won  ──► establish() for the other side, or a shot
```

## 2. Mechanism by mechanism

Each row answers the eight questions of the brief. "Tactics" are the five sliders (mentality, pressing,
tempo, directness, width) as 0..1.

### A. Pass candidate generation (`passing::generate_options`)

* **Input:** the carrier's *perceived* teammates (vision range / field of view / awareness radius, `perception.rs`)
  and opponents; ball position; the passer's `Tech`.
* **Criteria:** one candidate **per teammate**, aimed at the teammate's position led by his current velocity
  (`solve_pass_with`: three fixed-point iterations, lead capped at 2.5 s × 0.85). A candidate is dropped only if out
  of range or if the flight would end in the passer's own goal.
* **Assumptions:** a pass is *to a player*; the receiver is where he is or where his present speed takes him.
  There is no notion of a pass into empty space.
* **Tactics consulted:** tempo (arrival speed), directness (ideal distance, weight of progress and role).
* **Output:** `PassOption` list sorted by score. Continuous.
* **Downstream:** the best *viable* option competes with carry, hold and shoot.
* **Tests:** `tests/football_passing.rs` (20).
* **Defect (F4.2-D1):** a runner who stands level with the last defender is only ever offered the ball *at his
  feet*; the space behind the line is not a destination. See B and §3.

### B. Receiver selection and scoring (`PassWeights`, defaults `completion 1.0, progress 0.55, space 0.20, role 0.10, distance 0.15, backward 0.15, risk 0.80`)

* **Score** = completion + progress·(0.5+directness)(0.8+0.4·mentality) + space + role + distance − backward − risk − giver − offside.
* **Progress** is metres gained along the attack axis over 30 m, clamped to ±1. It does not know where the goal is,
  how many defenders stand between the aim point and the goal, or how crowded the route is.
* **Completion** = (1 − lane risk) · exec(distance, passing) · receive(first touch, pressure at the aim). Nothing asks
  whether the *receiver* can reach the aim before a defender does.
* **Downstream:** a short, safe pass to a marked striker (completion 0.76, progress +0.06) scores 1.03; a ball into the
  space behind him does not exist as an option. Measured in §3: at +0.9 s the best option is the short one in every
  seed inspected, with a high line and with a deep one.
* **Defects:** F4.2-D1 above; **D2** no estimate of what the destination is worth (danger given defenders goal-side),
  so a high line is not *more* attractive to attack than a deep one except through the linear `progress` term;
  **D3** no penalty for losing the ball where the team has nobody behind it.

### C. Evaluation of free space

* **For a pass:** `space = nearest perceived opponent to the aim / 10 m`, weight 0.20 (so at most 0.20 of a score
  that is typically 0.5 to 1.0). **For a carry:** `free_distance` along five directions, opponents within 3 m of the
  line block it. **Off the ball:** nearest opponent / 8 m.
* Only distance to the *nearest* opponent counts; the number of defenders goal-side, and their closing speeds,
  do not. Continuous.

### D. Forward runs and support movement (`offball::plan_attack`)

* Every 500 ms each off-ball attacker scores candidate points from the behaviours his role allows, and the best
  runs win up to `max_runners = 3` slots. `ForwardRun` and `DiagonalRun` candidates are placed at `line_s + 3` /
  `+ 1`.
* **`stay_onside` then clamps every target to the last defender's line** (`s_max = line_s − 0.5`). So a runner
  waits on the shoulder of the last defender and never goes into the space behind him, whatever the line height.
  The offside rule is already enforced where it matters, at the moment of the kick, by the passer's own offside
  reading (`p_off`, M9); the clamp is a second, stricter, rule.
* Tactics: directness and the player's `forward_runs` scale the run score; mentality scales `HoldDepth`.
  Phase-dependent (only while the team has the ball).
* **Defect (F4.2-D4):** with D1, the high line is never attacked by a run *or* a pass, so "space behind a high line"
  is not an affordance the simulation can use.
* **Tests:** `tests/m7_offball.rs` (13), `m10_behaviour.rs`.

### E. Defensive marking and coverage (`defend::plan_defence`)

* Threat = 0.45·nearness to goal + 0.25·speed towards goal + 0.20·free space + 0.10·ball proximity (M11: judged at
  the position `predict_s` = 1.2 s ahead; in the M12 mode). Top `max_threats` = 4 are marked, one marker each, at
  most 2 back-line players may leave the line. Hand-off between markers (M11/M12). Untasked defenders keep their
  **formation anchor** (zonal), which is where F4.1's mentality term acts.
* The M11 *compact block* (`m11.defence.shape`) and the pressure-aware block (`m12.pressure_shape`) are **off in
  the production configuration** ("measured and kept opt-in: more shots", `full_match_m11` comment). Consequence:
  in OFM the only things that make a defence "compact" are the formation anchors and the F4.1 mentality offset.
* Marking assignments do not depend on how many defenders there are. With ten men the same four threats are
  marked by fewer bodies, so the planner degrades only through `defenders.len()`; there is no explicit numerical-
  inferiority reaction. Phase-dependent (out of possession).
* **Tests:** `m11_defence.rs` (19), `m12_defence.rs` (8).

### F. RecoverGoalside (`defend::shape_pass`)

* **It is dead code in production.** The label is emitted only by `shape_pass`, which runs only when
  `m11.defence.shape` is on. In `full_match_m12()` it is off, so no player carries the `RecoverGoalside` label in an
  OFM match. F4.1's description of "a `RecoverGoalside` task sends players back within about two seconds" does not
  describe the production behaviour; after a loss, the team returns by (i) the formation planner's
  `TransitionDefence` phase anchors for 4 s, (ii) marking and pressing orders, (iii) `order_factor` speeds.
* When switched on (tests, studies) its target is the formation anchor squeezed towards the pitch axis, not a point
  chosen from the opponent's options; the speed is chosen from the distance to the target (`Urgent` if > 14 m), not
  from the individual's ability or condition. Ability enters only through the physical profile (acceleration, top
  speed, fatigue-scaled in M9) that limits any movement.
* **Defect (F4.2-D5):** recovery is a function of the *distance to the anchor*, not of the *threat that has to be
  covered*; there is therefore no difference between a full-back caught high with a runner in his channel and one
  with nobody to mark.

### G. Transition timing after possession loss

* `since_loss_ms` feeds only `plan_defence`: for `counterpress_ms` = 3.5 s the nearest players press (range up to
  12 m), then for `recover_ms` = 6 s the team recovers without re-pressing unless the carrier is within 7 m.
  The loser's formation planner switches phase for `transition_ms` = 4 s. These are fixed windows, not functions of
  how far out of position the team is.
* Continuous after the windows, phase-dependent inside them.

### H. Pressing triggers and success

* Primary presser: nearest by reach time within `8 + 22·pressing` m (production: the M10 leash / trigger /
  cooldown control is off). Second (pressing ≥ 0.35) contains 3.5 m goal-side; third (≥ 0.7) covers 7 m behind;
  one extra defender cuts the lane to the most dangerous receiver (≥ 0.3).
* **Defect (F4.2-D6, found by tracing a pressed build-up):** while a pass is in flight the planner's "carrier" is
  the **passer** (`team_tasks`: `(None, Some(f), _) => (f.side, f.passer, Some(f.target))`). The press therefore keeps
  closing the player who has just let go of the ball and re-targets only after reception plus up to 400 ms of
  planning interval. Pressure never arrives with the ball.
* **Pressure on the carrier** is `pressure_from(d1, d2)` = distance-only (nearest two opponents within 6 m). It lowers
  the hold utility, penalises carries, discounts the backward-pass penalty and widens the kick error by up to 50 %.
  It does not depend on how fast the opponent is closing, and a low-composure player is not hurried more than a
  calm one (**D7**).
* **Success** comes from duels only (`duel_win_probability`, tackling vs dribbling/strength; no dependence on how
  many defenders are around) and from interceptions of passes. Pressing intensity therefore moves *where
  defenders stand*, and its benefit depends on D6/D7.
* **Cost** is real: hsr + sprint distance of the pressing side rises 45 % from pressing 10 to 85 in §3-C. The
  benefit side was flat (builder loses the ball in 10 to 16 % of builds at every pressing level).
* **Tests:** `f4_tactics.rs` (`high_pressing_means_more_pressure_and_more_energy`), `m10_behaviour.rs` (b), `m11_defence.rs` (g).

### I. Chance generation and shot selection (`shooting::evaluate_shot`, `decision::decide_ext`)

* `shoot = 3.2·quality − 1.30 + 0.40·(risk_taking − 0.5)`, with quality = exp(−d/13)·angle·skill·lane·keeper·pressure.
  A shot is only on offer within 32 m and with quality ≥ 0.035. It competes, in the same units, with pass scores
  (typically 0.5 to 1.0) and carry utilities (0.3 to 0.9). At 10 m, central, unpressed, shoot ≈ −0.1.
* **Observation (not changed in this run, see the report §Remaining limitations):** in the staged situations a
  carrier with a free run walks the ball into the penalty area and along the by-line without shooting (box
  entered in 79 to 93 % of 12-second sequences, a shot taken in 2 to 6 %). Over whole matches the mean shot distance
  is 10.8 m and 65 % of shots are on target, which says that the engine's chances are mostly taken after a long
  carry, from very close. This is an existing calibration of M7 to M12 and a separate defect; fixing it would
  rewrite the shot count of every earlier mode's golden digest, so it is recorded, not edited.

### J. Numerical superiority and inferiority

* Nothing in pass selection, carry choice, off-ball movement or marking counts players. A dismissal removes a body
  from the formation planner (`mgmt_vacate`, shape rebuilt by the team's line) and from the defender list.
  In §3-E an attack against nine produced more forward passes (0.18 → 1.0 per 20 s) but the same shot rate (0.10 →
  0.11): the spare man shows up as a lane, not as a chance.

## 3. Diagnostic experiments (unmodified engine, `4057c67`)

`tests/f42_diagnostics.rs`; 300 seeds per cell; all situations are staged mid-match with `stage_situation`
(§4) and then played by the unmodified engine. The seed varies the engine's draws (first touch, duels, kick error);
the layouts are identical. Raw output: `docs/f42/` in the M12 repository.

| # | Situation | Observed (baseline) | Failing link | Justified correction |
|---|---|---|---|---|
| A | Team loses the ball with its line at 28 / 38 / 48 m; opponent has a carrier and two forwards level with the line | First pass gains ≥ 8 m in 0 / 0 / 2.7 % of cases; shots in 12 s 5.7 / 2.3 / 3.0 %; defenders goal-side of the ball at +4 s 2.8 / 3.2 / 4.1 | D1, D2, D4: the space behind a high line is not a destination for a pass or a run | Through-ball candidates, destination danger, receiver race; runs allowed beyond the line while the carrier is free |
| B | Attack vs organised compact block / standard width / disorganised / exposed | shots 0.08 / 0.17 / 0.11 / 0.03 per 20 s; forward passes 0.5 / 0.3 / 0.6 / 0.1 | Partly works (compact < standard); the "exposed" layout in this run puts the carrier on the defenders and is not a fair comparison (see report) | None in this layer; recorded |
| C | Away keeper builds against a press of 10 / 50 / 85 / 100 | Builder loses the ball in 10.7 / 15.7 / 11.7 / 13.0 % of builds; pressure reaches the ball after 12.0 / 9.0 / 9.4 / 9.5 s; pressers' hsr+sprint 758 / 914 / 1079 / 1082 m | D6, D7: cost rises with intensity, benefit does not | Press the receiver in flight; pressure that counts closing speed and degrades a hurried pass |
| D | Builder has beaten the first line (0 / 2 / 4 / 6 pressers beyond the ball) | Shots 4.3 / 1.0 / 4.0 / 11.0 % in 16 s; goal-side of ball at +4 s 6.0 / 6.9 / 3.6 / 4.1 | Exposure is visible (more shots with 6 beyond) but small | Covered by A's mechanism |
| E | The same attack v 11 / 10 / 9 | shots 0.10 / 0.13 / 0.11; forward passes 0.18 / 1.10 / 1.03; box 3.3 / 7.7 / 4.3 % | J: the extra man opens lanes, which the attack then does not turn into chances | Space-aware option scoring (A) |

## 4. Instrumentation added

* `Simulation::stage_situation`, `MotionRuntime::stage_situation`, `FootballRuntime::stage_possession` (M12): put
  both teams at prescribed places and hand the ball over through the engine's own `establish`. Diagnostic only;
  refused outside open play, and a session that used it cannot be checkpointed (`inject_scenario`).
* `Simulation::inspect_pass_options_detailed`: the score parts of every option.
* `apply_f42_flag` and the `F42_FLAGS` environment variable of the study harnesses, so every mechanism is a named,
  separately switchable `M12Config` field (all off by default: earlier modes stay bit for bit).
