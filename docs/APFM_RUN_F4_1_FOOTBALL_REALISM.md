# APFM Run F4.1: football realism, balance and interactive-match stabilisation

Branches: OFM `claude/run-f4-1-football-realism` (from the verified F4 head `a69f48fb`), M12
`f4.1/football-realism` (from the verified F4 head `c17d5f9`). Nothing was merged into `apfm/main` or an
integration branch. No save and no FM24 database was touched: every test builds its own career in a temporary
directory. The M12 licence edits that were already uncommitted (`Cargo.toml`, untracked `LICENSE`/`NOTICE`)
were left alone and are in no commit.

## 1. Executive verdict

**Done and verified:** mentality now shapes where the team stands; restarts use the formation in force;
a single post-match condition charge reflects the match's physical load; a career can be saved in the
middle of an interactive match and resumed; the dependency on M12 is pinned by Git revision; and two bugs
that made roughly **1 interactive match in 4 impossible to finish** are fixed.

**Not done, or only partly:** the engine does not turn a team's exposure into chances against (§3.3), so
an attacking mentality is not paid for in shots conceded; the shared manager policy covers urgency only,
not substitutions or formations (§6); the season-length scenario (§9, scenario 6) and a click-through of
the real desktop app were not run (§11); the `Remaining limitations` list is long and honest (§12).

| Acceptance rule | Status | Where |
|---|---|---|
| Mentality changes team shape and exposure | **Met** (positions, support, goal-side cover) | §2, §3 |
| Attacking mentality carries a defensive cost in *outcomes* | **Not met**: structural cost only | §3.3 |
| Defensive mentality protects | **Not met**: compact, but no protection in goals against | §3.3 |
| Pressing costs energy and position | **Met** | §3.2, §5 |
| No setting dominates; quality beats tactics | **Met** for quality; dominance grid in §3.4 | §3.4 |
| One authoritative fatigue update, pressing not free | **Met** (hybrid) | §5 |
| Computer managers consistent with the autonomous one | **Partly**: urgency shared, the rest documented | §6 |
| A saved match resumes; no duplicate finalisation | **Met** | §7 |
| A match that reaches full time can be finalised | **Met for every seed tested** after §8.1 (20 regression seeds, 90 surveyed; was ~75 %); not a proof for all seeds | §8 |
| Reproducible dependency | **Met** (pinned, verified on a clean clone) | §8.5 |

## 2. Tactical realism before and after

**Before (F4):** mentality reached only pass selection and the off-ball "hold depth" weight. With 200 pairs,
an attacking mentality changed none of: shots conceded, counter-attack shots conceded, back-line depth,
front-line depth, team length (F4 report §6.3).

**After:** `FormationPlanner::target` adds a bounded longitudinal term per player
(`mentality_offset`): `mentality(-1..1) × reach(line) × commitment(role) × phase weight`.

* Reach at the extremes: 10 m defence and midfield, 7 m attack; team depth stretch ±5 %.
* Commitment by role: full-backs and wing-backs 1.0, wide and attacking midfielders 1.0, central
  midfielder 0.85, holding midfielder 0.4, centre-back 0.25, strikers 0.5, wingers 0.6.
* Phase weight: with the ball and on the break everybody joins in; an attacking side keeps 0.55 of its reach
  without the ball (it does not drop all the way: that is the exposure) and 0.8 when the ball has just been
  lost (late to recover); a defensive side drops fully without the ball.
* **A defensive mentality is compactness, not collapse:** centre-backs hold their line exactly (0.0) and
  full-backs tuck in at 0.5. My first version dropped the centre-backs toward the box and handed the opponent
  in-box shots (+2.9 of +3.1 extra shots); changing that cut shots conceded at the extreme setting from +3.65
  to +1.70 on the same 150 seeds.
* Balanced (50) adds exactly nothing (a test compares it with the term switched off, in every phase).
* **Visibility criterion, fixed before any outcome was seen:** the two play styles a player can reach from
  the UI that differ most (Defensive = mentality 30, Attacking = 70) must differ by at least 5 m of midfield
  depth with the ball. `[6, 6, 4]` gave about 4 m and failed; `[10, 10, 7]` gives about 7 m and was adopted.
* It is a **mode switch** (`M12Config::mentality_shape`, on in `full_match_m12()` only). Turning it on in
  every mode broke three golden-digest suites of earlier missions (`m10_matches`, `m12_audit`,
  `m10_management`); gating it follows the repository's own rule that earlier modes stay bit-identical, and
  all three pass unmodified. The same holds for restarts (`restart_in_current_shape`).

Structural tests (`tests/f41_mentality_shape.rs`, 10): balanced is the unmoved reference; with the ball the
midfield and wide defenders follow the mentality monotonically; full-backs commit far more than
centre-backs; an attacking side does not retreat all the way without the ball; a defensive side is deeper
and shorter; more teammates ahead of the ball when attacking; every term is bounded and lines keep their
order; mentality does not override a line-height instruction; a defensive mentality keeps the back line;
only the M12 mode enables it. Five of them fail when the mechanism is removed (checked by zeroing it); the
other five are invariants that hold either way, by nature.

## 3. Statistical comparisons

Design: 300 paired seeds per principal comparison; identical teams (level 65) and seeds; only the home team's
orders differ; the away team stays balanced. 95 % intervals are paired-difference intervals; `dz` is the
paired effect size (mean difference over its standard deviation). Instrument: the engine's read-only probes
plus the canonical event stream. There is no xG in the engine, so the "danger" measures are existing ones:
in-box shots, shots on target, and chances struck with at most two defenders goal-side ("open chances").

**What is asserted and what is only reported was fixed before the runs:** structural effects (support,
midfield depth with the ball, goal-side cover after a loss, team length) are asserted; shots, danger,
goals, possession and points are reported. A paired points difference has a standard error of about 0.085 at
300 seeds, so points cannot resolve effects worth less than about 0.2.

### 3.1 The mentality ladder (pressing 50), home vs balanced

Home team at mentality 10 / 30 / 70 / 90 against the same team at 50. (UI play styles reach 30 to 70: Defensive 30, Balanced 50, Attacking 70.)

| metric (mean paired difference, 95% CI) | very_defensive | defensive | attacking | very_attacking |
|---|---:|---:|---:|---:|
| shots for | -2.373 [-2.954, -1.793] | -0.853 [-1.412, -0.295] | +0.533 [-0.058, +1.125] | +2.353 [+1.761, +2.946] |
| shots conceded | +0.910 [+0.306, +1.514] | +0.317 [-0.255, +0.888] | -0.957 [-1.509, -0.404] | -1.153 [-1.755, -0.552] |
| shots on target conceded | +0.823 [+0.319, +1.327] | +0.420 [-0.043, +0.883] | -0.483 [-0.913, -0.053] | -0.533 [-0.999, -0.068] |
| open chances conceded | +0.483 [-0.063, +1.030] | +0.000 [-0.511, +0.511] | -0.673 [-1.176, -0.171] | -0.883 [-1.425, -0.342] |
| counter-attack shots conceded | +0.147 [-0.026, +0.320] | -0.097 [-0.268, +0.075] | -0.047 [-0.209, +0.116] | -0.087 [-0.241, +0.067] |
| won in opp. final 40 m | -1.373 [-1.846, -0.901] | -0.813 [-1.278, -0.349] | +0.787 [+0.282, +1.291] | +2.577 [+2.050, +3.104] |
| midfield depth with ball (m) | -7.130 [-7.240, -7.020] | -3.129 [-3.241, -3.016] | +3.179 [+3.057, +3.301] | +5.851 [+5.725, +5.978] |
| players ahead of ball | -0.219 [-0.238, -0.199] | -0.131 [-0.152, -0.111] | +0.165 [+0.144, +0.186] | +0.317 [+0.296, +0.339] |
| team length w/o ball (m) | -2.854 [-2.985, -2.723] | -1.494 [-1.611, -1.377] | +1.038 [+0.913, +1.163] | +2.246 [+2.110, +2.382] |
| goal-side 4 s after loss | +0.281 [+0.239, +0.324] | +0.130 [+0.091, +0.168] | -0.199 [-0.241, -0.157] | -0.342 [-0.385, -0.300] |
| energy (eu) | +1594.236 [+971.983, +2216.489] | -479.625 [-1077.834, +118.584] | +648.194 [+40.034, +1256.355] | +2146.453 [+1543.548, +2749.359] |
| condition at end | -0.007 [-0.009, -0.004] | +0.002 [-0.000, +0.005] | -0.003 [-0.006, -0.001] | -0.011 [-0.014, -0.009] |
| goals for | -0.130 [-0.317, +0.057] | -0.073 [-0.252, +0.105] | +0.083 [-0.102, +0.269] | +0.187 [+0.007, +0.366] |
| goals against | +0.053 [-0.125, +0.232] | -0.040 [-0.218, +0.138] | -0.087 [-0.269, +0.096] | -0.023 [-0.210, +0.164] |
| points | -0.103 [-0.307, +0.101] | -0.043 [-0.250, +0.164] | +0.187 [-0.018, +0.391] | +0.190 [-0.018, +0.398] |

### 3.2 Mentality combined with pressing

Home team at (mentality, pressing) against balanced (50, 50). Attacking = 70, defensive = 30, high press = 85, low press = 25.

| metric (mean paired difference, 95% CI) | attacking_high_press | attacking_low_press | defensive_high_press | defensive_low_press |
|---|---:|---:|---:|---:|
| shots for | -0.967 [-1.529, -0.404] | +2.610 [+1.979, +3.241] | -2.273 [-2.849, -1.698] | +1.203 [+0.567, +1.839] |
| shots conceded | +0.780 [+0.181, +1.379] | +5.673 [+5.033, +6.313] | +1.793 [+1.230, +2.357] | +6.910 [+6.257, +7.563] |
| shots on target conceded | +0.340 [-0.123, +0.803] | +5.887 [+5.328, +6.446] | +1.123 [+0.674, +1.573] | +6.807 [+6.238, +7.376] |
| open chances conceded | -0.933 [-1.432, -0.435] | +7.437 [+6.820, +8.053] | -0.810 [-1.295, -0.325] | +8.320 [+7.697, +8.943] |
| counter-attack shots conceded | +0.113 [-0.065, +0.292] | -0.237 [-0.395, -0.078] | +0.217 [+0.031, +0.403] | -0.217 [-0.379, -0.054] |
| won in opp. final 40 m | -0.363 [-0.860, +0.134] | +0.763 [+0.261, +1.266] | -1.437 [-1.923, -0.950] | +0.043 [-0.464, +0.550] |
| midfield depth with ball (m) | +2.373 [+2.249, +2.498] | +4.869 [+4.765, +4.972] | -3.568 [-3.685, -3.450] | -1.598 [-1.700, -1.497] |
| players ahead of ball | +0.142 [+0.121, +0.163] | +0.173 [+0.153, +0.193] | -0.167 [-0.187, -0.146] | -0.165 [-0.182, -0.148] |
| team length w/o ball (m) | -1.967 [-2.127, -1.806] | +4.108 [+4.010, +4.205] | -4.110 [-4.266, -3.953] | +1.205 [+1.102, +1.307] |
| goal-side 4 s after loss | -0.394 [-0.440, -0.349] | +0.256 [+0.213, +0.299] | +0.016 [-0.028, +0.060] | +0.494 [+0.451, +0.537] |
| energy (eu) | +13465.173 [+12795.824, +14134.522] | -20586.005 [-21095.621, -20076.389] | +11625.846 [+10994.460, +12257.232] | -20400.021 [-20905.844, -19894.199] |
| condition at end | -0.056 [-0.059, -0.054] | +0.085 [+0.083, +0.087] | -0.048 [-0.050, -0.045] | +0.085 [+0.083, +0.087] |
| goals for | -0.083 [-0.267, +0.100] | +0.350 [+0.151, +0.549] | -0.120 [-0.306, +0.066] | +0.163 [-0.029, +0.356] |
| goals against | +0.027 [-0.156, +0.209] | +0.327 [+0.129, +0.524] | +0.057 [-0.131, +0.245] | +0.500 [+0.312, +0.688] |
| points | -0.057 [-0.265, +0.151] | +0.033 [-0.176, +0.243] | -0.063 [-0.276, +0.149] | -0.177 [-0.389, +0.035] |

### 3.3 What the numbers say, including what I did not manage to fix

* The structural effects are large and clean (dz 1 to 8): midfield depth with the ball, players ahead of the
  ball, goal-side players after a loss, team length.
* **They do not convert into chances against.** An attacking side wins more possession high up the pitch and
  concedes *fewer* open chances. To tell "the engine cannot exploit space" from "my parameters are too
  small", I ran a dose-response at 2.5× the commitment on 150 seeds: shots conceded and open chances conceded
  still fell and counter-attack shots did not rise. The cause is in the engine's attack, not in the
  planner: pass scoring weighs receiver space, but a `RecoverGoalside` task sends players back within about
  two seconds, and a counter is a small share of chances (about 1 shot in 13). Making the opponent's carrier
  exploit a high line would mean recalibrating M10 to M12 football, which F4 already warned against and I did
  not do silently.
* **Consequence, from the table above:** at the UI-reachable settings an attacking mentality gains
  territory and concedes slightly fewer chances (open chances −0.67), with no significant effect on points
  (+0.19, CI crosses 0); at the engine's extreme (90) it also creates +2.4 shots and is borderline on points.
  A defensive mentality (30) now does what its name says on the attacking side (−0.85 shots, −3.1 m of
  midfield depth with the ball, fewer players ahead of the ball) and leaves chances against unchanged
  (+0.3 shots, open chances 0.0): compact, no longer collapsing, but not protective either. This is the main
  open football issue (§12, item 1); the recommended fix is on the attack side, tested the same way.
* Pressing is the larger lever. Low pressing saves about 14 % of the energy (+8.5 points of condition at
  the end) and turns the game open: shots conceded +5.7 to +6.9 and open chances conceded +7 to +8, with
  shots for +1.2 to +2.6, so it is a real trade, not a free saving. High pressing costs 8 to 9 % more energy
  (−5 points of condition) and, in this engine, does not win possession higher up the pitch (won in the
  opponent's final 40 m: −0.4 and −1.4, not significant for the first) and lowers the side's own shots.
  It is "not automatically superior to low pressing", as asked, but its upside is not visible in these
  measures either (§12, item 1).

### 3.4 No setting dominates; quality beats tactics

Strong team (attribute level 72) against a weak one (58), 300 paired seeds, the strong team playing each
mentality; the first row is two equal teams (65 against 65) for reference.

| Strong team's setting | points | win | draw | loss | goals for | goals against |
|---|---:|---:|---:|---:|---:|---:|
| (equal teams, balanced) | 1.31 | 34 % | 28 % | 38 % | 1.37 | 1.42 |
| very defensive (10) | 2.54 | 80 % | 15 % | 5 % | 2.80 | 0.62 |
| defensive (30) | 2.56 | 80 % | 15 % | 5 % | 2.95 | 0.67 |
| balanced (50) | 2.74 | 89 % | 7 % | 4 % | 3.35 | 0.45 |
| attacking (70) | 2.70 | 86 % | 11 % | 2 % | 3.12 | 0.57 |
| very attacking (90) | 2.79 | 91 % | 7 % | 2 % | 3.43 | 0.47 |

Quality is worth +1.43 points against equal opposition; the whole range of tactical settings is worth 0.25.
The strong team is stronger in every setting and the weak team still wins 2 to 5 % of the time. The gap I chose
(14 attribute levels) is large; a league-typical gap would be smaller, and I did not run one. Asserted in
`tactics_matter_but_quality_matters_more_over_300_pairs`: it passed.

**Dominance grid** (`no_tactical_setting_dominates_over_300_pairs`): nine home settings against three opponent
styles, 300 seeds per cell, mean points per match for the home team.

| Home setting | vs balanced | vs attacking + high press | vs defensive + low block |
|---|---:|---:|---:|
| balanced | 1.31 | 1.38 | 1.48 |
| very defensive (10) | 1.21 | 1.10 | 1.63 |
| defensive (30) | 1.27 | 1.31 | 1.59 |
| attacking (70) | 1.50 | 1.49 | 1.64 |
| very attacking (90) | 1.50 | 1.66 | 1.70 |
| attacking + high press | 1.25 | 1.26 | 1.64 |
| attacking + low press | 1.34 | 1.52 | 1.52 |
| defensive + high press | 1.25 | 1.27 | 1.36 |
| defensive + low press | 1.13 | 1.20 | 1.35 |

The assertion (no setting is significantly better than balanced against *every* opponent) **passes**. It does
not settle the question. Attacking and very attacking are ahead of balanced in **all three columns**
(+0.19, +0.11, +0.16 and +0.19, +0.28, +0.22); no single cell is significant at this sample size (a paired
points difference has a standard error of about 0.085 and the three columns are not independent tests of a
design that was powered for it), but three consistent signs is the pattern a weak dominance would produce.
Defensive and low-pressing settings are at or below balanced against every opponent. I read this as the
§3.3 finding showing up in the aggregate, and I list it as unresolved (§12, item 1) rather than as a pass.
Everything else is a real trade: each of the high-press and low-press settings wins in one column and loses
in another.

## 4. Priority A and F-A: the mechanisms that changed

| Change | Where | Test |
|---|---|---|
| Mentality positioning term | `formation.rs` (`mentality_offset`, `FormationParams::m12`) | `f41_mentality_shape.rs` |
| Restart in the formation in force (halftime and after a goal) | `runner/management.rs` (`live_kickoff_layout`) | `f41_restart_geometry.rs` (4) |
| A period can end while a restart is pending | `runner.rs` (`end_period`) | `end_of_period_tests` |
| In-possession shape and support probes | `defprobe.rs` | used by every study |

**Halftime geometry (F4 limitation 2):** the layouts were computed once from the input lineup, so after any
formation change both the second-half kickoff and every kickoff after a goal used the old shape. They are now
built from the live shapes. A control test shows the unchanged case gives exactly the old layout, and another
that earlier modes keep their pinned behaviour.

**Period end during a restart (found by the dominance grid):** a goal a moment before the whistle left the
state machine in `KickoffPending`, which refuses half-time and full time, so the whole match failed (one case
in about 8,000 matches). The restart whistle is now blown first.

## 5. Fatigue: the comparison and the decision

Measured on real M12 matches (`examples/f41_fatigue.rs`, 60 seeds, about 600 starters per row); "M12" is
the share of the aerobic reserve spent, in points of the 0 to 100 condition; "OFM" is the existing
`apply_match_wear`, 40 × (1 − 0.4·stamina) × minutes/90.

| Case | M12 spent | OFM charge |
|---|---:|---:|
| 90', pressing 10 / 50 / 90 | 22.7 / 32.7 / 38.3 | 29.6 / 29.6 / 29.6 |
| 90', defenders / midfielders / forwards (fixture players) | 13.8 / 43.9 / 47.9 | identical |
| 90', low / mid / high stamina | 35.9 / 33.0 / 29.3 (×1.22) | 30.9 / 29.7 / 28.3 (×1.09) |
| Spread between players (sd) | 16 | 1 |
| Substitute, about 19' | 12.3 | 6.7 |
| Starter replaced at about 70' | 31.6 | 23.4 |
| 90' arriving at condition 50 | 29.6 (same as fresh: spending is additive) | 29.6 |
| Extra time, 120' | 39.9 | 39.4 |

| Criterion | OFM minutes model | M12 expenditure |
|---|---|---|
| Realism and player differentiation | none beyond stamina | responds to attributes and workload |
| Tactical consequences | **none: a high press is free in the career** | pressing +17 % / −31 % |
| Career consistency | the whole calendar (recovery, rotation thresholds, training) is calibrated around about 29 points a match | absolute spread is not usable: 3× between lines |
| Save compatibility | unchanged | needs a report field (serde default) |
| Computational cost | none | already computed |
| Double counting | n/a | M12 already applies the carried-in condition during the match; writing its end state as well would charge twice |

**Decision: Option 3, a carefully defined hybrid.** The OFM formula stays the baseline. The engine's measured
load decides only whether *this match* was harder or easier than a neutral one:

`depletion = baseline(stamina, minutes) × clamp(load / neutral(minutes, stamina), 0.6, 1.5)`,
shrunk toward 1 for short appearances (`minutes/45`), goalkeepers excluded.

* One writer: `deplete_match_stamina` (`turn/post_match.rs`). The engine's end condition is never written
  to the player as well. The report carries `physical_load` (`#[serde(default)]`, `None` for the legacy
  engine, so friendlies and old records behave exactly as before).
* Pressing 90 costs about 17 % more, pressing 10 about 30 % less; a bad match cannot cost more than 1.5× or
  less than 0.6× the baseline, so there is no spiral from one fixture.
* **Calibration bug the guard test caught:** my first reference was a per-position table measured on the
  engine's own fixture players (3× between lines). On career players adapted by OFM the neutral workload is
  nearly equal across lines (0.00244 / 0.00255 / 0.00233 per minute); with the table, midfielders were charged
  30 % too little (mean ratio 0.71). The reference is now one neutral rate (0.0025 per minute at stamina 64),
  and `neutral_matches_cost_the_reference_load` checks the mean per line stays within 0.85 to 1.2 (measured
  1.04 / 1.11 / 0.95). Differences between players then come from their attributes, which is the intent.
* Season check (reasoning, not a simulation): neutral cost is unchanged (29.6), recovery on a rest day is
  about 8 to 12 points, so a once-a-week schedule is stable and a twice-a-week one drifts down as it did
  before. A pressing-90 side pays about 35 per match and rotates sooner; the 1.5× cap and the existing slower
  recovery below 30 keep that bounded. A multi-fixture simulation with the real training recovery was not run.
* Verification limit: the neutral rate was measured on synthetic adapted players. It should be re-measured on
  FM24 safe copies before it is trusted for a real world; the guard test is the tool.

## 6. AI manager policy

| | M12 autonomous (`runner/management.rs`) | OFM interactive (`interactive_match/ai.rs`) |
|---|---|---|
| Urgency | `need_v2`: goals behind against minutes left, lead only urgent once the end is near | **now the same function** |
| Tactics | slider targets `tactical_targets_v2`, hysteresis, step limits, opponent momentum | discrete play style, pressing, tempo from `need`, cooldown |
| Substitutions | whole bench scored by utility (quality × fit × condition, tactical term, costs) | injured first, tired late, one tactical swap |
| Formation | `formation_utility` over all presets | only after a dismissal |
| Acts | at every dead ball, inside the engine | at checkpoints (break, dismissal, injury, 10 minutes) |

* **Decision:** share one *judgement*, keep two *executions*. `need()` in OFM's policy is now
  `2 × (chase − protect)` from `manager::need_v2` with an extra-time horizon (tests: equality with the M12
  function on a table of states; a goal down is urgent only when time is short; extra time). A club therefore
  no longer feels a different urgency depending on whether a match is watched.
* **Not consolidated (documented divergence):** substitution scoring, formation choice and opponent-momentum
  sensing remain two implementations. Folding them into one contract would change the discretionary
  decisions of every simulated match, which the golden suites pin, and needs its own calibration run.
* Required behaviours (red cards, injuries, substitutions, score and time, available players, no knowledge of
  the future, no thrashing) are covered by the existing 17 policy tests plus 3 new ones; none of the policy
  reads the future or the engine.

## 7. Mid-match save and reload

* **Storage:** table `interactive_match_checkpoint` (migration v064), one row, written in the *same
  transaction* as the career. Envelope: schema version, SHA-256 of the payload, competition and fixture
  identity. Payload: the engine checkpoint (input, configuration, journal of accepted commands with the tick
  each was applied at), the controller's memory (authority, assistant state, order ledger, AI cursors) and the
  round baselines. About 100 KB at minute 30 with 200 commands; no frames, no positions.
* **Why the round baselines travel:** starting a live session plays the rest of the day and captures the table
  before the round; after a reload those fixtures are already played, so re-deriving the baseline would read
  every delta as zero.
* **Authority:** saving is allowed during an interactive match and still refused during any other live match.
  Finishing clears the row in the same save as the finalised fixture, so a finished match can never be
  resumed and applied again; a stale checkpoint whose fixture is no longer scheduled is dropped silently.
* **Restore:** the engine replays the journal and must reproduce the saved world digest; a different engine
  build, a damaged journal, an altered payload, another envelope version or malformed text are each detected
  and discarded; the career still loads, the fixture stays scheduled, and the player gets **one** translated
  notice (`be.error.interactive.savedMatchDiscarded`, 12 locales). Old saves have no row and load as before.
* **UI:** after a load the menu asks `get_resumable_match` and opens the match route straight into the
  managing screen; `take_match_restore_notice` shows the one-time notice.
* **Tests:** `saved.rs` (8: same match resumes, restore is deterministic, repeated command id applies
  nothing, garbage, other version, digest mismatch, completed match cannot be stored); `interactive_match_repo`
  (5); application level (5): a match 20 minutes in with an accepted order is saved, reloaded into a fresh
  state, resumes with the same clock, score and tactics, finishes once, a second finish is refused, the next
  save holds no checkpoint, no replay is stored and the checkpoint is under 400 KB; a damaged row is discarded
  with one notice; an old career loads unchanged; a non-interactive live match still blocks saving; frontend
  (2): resume navigation and the notice.
* **Compatibility guarantee:** adding the table is the only schema change; the older-release fixtures in the
  `db` tests had to learn to drop it. A checkpoint written by a different engine build is refused rather than
  resumed as a different match, so **a build that changes M12 football invalidates saved in-progress
  matches** (they restart from kick-off, with the notice).

## 8. Corrected F4 issues

### 8.1 A match could not always be finished (found, not in the brief)

Finalising 60 seeds with no orders at all on the **exact F4 M12 commit**: 15 failed (25 %), 12 with
`InvalidEvent("hit_woodwork")` and 3 with `PlayerOnWrongTeam`. F4's end-to-end test happened to use a seed
that avoided both. After the fixes 87 of 90 seeds finalised and the remaining 3 were the second cause; after
both, a regression test finalises 20 seeds including the ones that failed.

* **Woodwork:** the decoder did not know the shot outcome `hit_woodwork`; a shot that hits the frame is now a
  shot off target.
* **Own goals:** the projection refused them (`OwnGoalCannotBeProjected`) but a team-membership check fired
  first and hid that. An own goal now counts for the side it is credited to and credits nobody; it is in the
  timeline as a goal with no scorer and not in the scorers list. The UI therefore shows a goal without a name
  (limitation 6).

### 8.2 The rest of §8 of the brief

| Item | Result |
|---|---|
| A halftime restart geometry | Fixed (§4) |
| B goalkeeper substitution paths | Covered: bench keeper adapted as a keeper (F4 fix), injured or dismissed keeper replacement in M12 and in the AI policy tests (`an_injured_goalkeeper_...`); no new defect found |
| C record contents | `the_match_record_is_complete_and_consistent`: swapped players' minutes add up to the match, scorers never exceed the score and each has the goal in his own record, cards in the timeline equal cards in the statistics. (The substitution event's `player_id` is the player coming **on**.) |
| D post-match condition | One authoritative charge (§5) |
| E suspensions and injuries carry forward | They go through the same `apply_match_report` as every match; **not separately re-verified** on an interactive match with a forced red card (limitation 7) |
| F no large replays retained | Asserted: no replay row for an interactive match, checkpoint under 400 KB |
| G finalisation and retry paths | F4 tests still pass (retry keeps the finished match; two simultaneous finishes apply once); the checkpoint row is cleared with the finalised save |
| H reproducible dependency | §8.5 |
| I reviewer findings | The three golden-suite regressions I introduced were fixed by gating, not by rebaselining |

### 8.3 Tests I changed, and why

* `f4_tactics.rs`: the defensive-mentality assertion measured forward metres per completed pass, which fell
  when mentality only weighed pass selection. Once positions follow mentality, a deeper team covers more
  ground per forward pass (2.5 m against 1.6 m at mentality 10, with *fewer* long passes: 3.2 against 4.3 per
  match), so that sum stopped measuring progress. It is replaced by the depth at which completed passes are
  played, which is monotone (38.2, 41.4, 43.4, 45.8, 47.6 m for 10, 30, 50, 70, 90); the attacking test
  asserts both. Both pass at 200 pairs.
* `db` tests: the version assertion uses `MIGRATION_COUNT`; the "older release" file builder drops the new
  table, as it drops the other later additions.
* No threshold was loosened to make a result pass. One scenario assertion (red card) was changed from points
  to chances after I computed that points cannot be resolved at this sample size; the reasoning is in §9.

### 8.4 Mistakes of mine worth recording

* The scenario harness first gave the baseline arm no split instant, so its metrics covered the whole match
  against twenty minutes for the treated arm. The tables looked absurd (491 passes against 105) and were
  discarded; every arm of a scenario now splits at the same instant.
* M12 commit `9fe0439` says it also changed the red-card assertion; it did not (my edit silently failed to
  apply, which the next scenario run exposed). `0b4855a` is the commit that made the change.
* One test had the substitution orientation backwards and one had a wrong expectation about idempotent
  command ids; both were my errors, not defects.

### 8.5 Reproducible dependency

`ofm_core` now depends on `apfm-match-lab` by `git` and `rev`; `Cargo.lock` records the exact source. The
repository is **private**, so resolving needs git access to it (a credential helper, `gh auth setup-git`, or a
CI token); no credential or URL token is in the repository. Local development against a sibling checkout:
copy `src-tauri/.cargo/config.toml.example` to the git-ignored `.cargo/config.toml` (a `paths` override, which
leaves `Cargo.lock` untouched, unlike `[patch]`). Verified: the override leaves the lockfile byte-identical;
a clean `git clone` resolves the pinned revision under `cargo fetch --locked` and `cargo tree --locked`.
CI needs the token configured before this branch can build there.

## 9. End-to-end football scenarios

Paired experiments (`tests/f41_scenarios.rs`, all `--ignored` studies). Every arm of a scenario shares the
seed and the match up to the decision and splits at the same instant; "after" columns cover the same minutes
in each arm. Scenarios 1 and 2 use the seeds where the state *really arose* in an unforced match (196 and 204
of 1,500). 95 % intervals are paired.

**Scenario 1: defending a 1-0 lead at 70'** (a side that has been attacking, mentality 70; the alternative is
mentality 30, pressing 35, a lower line).

| Arm | win | draw | loss | shots conceded after | goals conceded after |
|---|---:|---:|---:|---:|---:|
| keep attacking | 88 % | 12 % | 1 % | 2.51 | 0.18 |
| protect the lead | 77 % | 20 % | 3 % | 2.83 | 0.30 |

Protecting made things **worse**: points −0.235 [−0.38, −0.09], own shots −0.45, shots conceded +0.32, goals
against +0.12. It is a trade-off, not a guarantee in either direction (the lead is kept 77 to 88 % of the
time), but it is the opposite of the product principle "defensive should provide legitimate protection". The
cause is the §3.3 finding combined with the pressing result in §3.2: dropping the press opens the game more
than the deeper block closes it. A manager who protects a lead by pressing less is punished by this engine.

**Scenario 2: chasing a goal at 70'** (balanced; the alternative is mentality 70, pressing 65). No
significant effect on anything: shots +0.14 [−0.20, +0.48], shots conceded −0.17, points +0.05 [−0.07, +0.17],
win 2 % against 4 %. Chasing the game "alters commitment" (§3.1) but the relevant risk and reward are both
too small to see in twenty minutes.

**Scenario 3: a red card at 35'** (a central midfielder is sent off; 300 seeds, forced).

| Arm | win | draw | loss | shots for after | shots conceded after |
|---|---:|---:|---:|---:|---:|
| eleven men | 34 % | 28 % | 38 % | | |
| ten men, structure kept | 37 % | 26 % | 37 % | −1.10 [−1.50, −0.69] | +0.77 [+0.32, +1.23] |
| ten men, reorganised (4-5-1, mentality 40) | 36 % | 28 % | 36 % | +0.62 over "kept" | −0.43 over "kept" |

Ten men create about 13 % fewer chances and concede about 9 % more, and open chances conceded rise by 0.82
[+0.43, +1.22]; they also run less (condition at the end +1.1 points for the reorganised side). The result is
not predetermined (ten men win 36 to 37 %). **But final points did not fall** (+0.05 [−0.12, +0.22]):
the paired points difference cannot resolve an effect worth a fifth of a point at 300 seeds, and a 10 to
15 % swing in chances is worth about that. I therefore assert the powered measure (chances) and report the
points. I do **not** claim the disadvantage is as large as football's: ten men for 55 minutes would
normally cost more than this engine shows (§12, item 2). Both answers are legal and coherent; reorganising
helps modestly and significantly on open chances conceded (−0.42 [−0.82, −0.01]).

**Scenario 4: a team that pressed hard (pressing 90) for 70'**; the alternative brings on the three most tired
outfielders for fresh bench players. Condition at the end is +7.1 points better with fresh legs
[+7.0, +7.3]; shots and goals do not differ (shots conceded +0.16, points +0.03 [−0.07, +0.13]) and open
chances conceded are slightly higher (+0.21 [+0.01, +0.42]). The engine's fatigue effects bite late and at
high long-term fatigue, so twenty minutes with tired legs costs little; the physical benefit is real, the
tactical one is small here.

**Scenario 5: strong against weak** is §3.4.

**Scenario 6, a season of consecutive matches (fatigue, recovery, injuries, suspensions, rotation,
save/reload, statistics): not run.** What exists instead: save, reload, resume and single finalisation are
tested end to end (§7); consecutive fixtures use the existing finalisation, which the season-harness suites
exercise for the legacy path.

## 10. Performance

Not re-measured as a benchmark. The engine changes are position targets and a one-off layout at restarts;
the new probe samples (once per second, read-only) exist only when the probe is on. For reference, a probe-on
full match in the studies cost about 1.1 s on this machine against F4's 0.61 s headless measurement, which
is the probe and not a regression of the headless path; I did not run `examples/f4_perf.rs`.

## 11. Verification

| Suite | Result |
|---|---|
| M12 `cargo test --release --no-fail-fast` | 594 passed, 0 failed, 8 ignored (the `--ignored` studies). The first full run found one failure: the test that pins the closed list of switches of the M12 mode, which now includes mine; it was updated and passes |
| M12 `--ignored` studies run for this report | mentality ladder, mentality × pressing, strong vs weak, dominance grid, scenarios 1 to 4: all passed their assertions (scenario 3 after its assertion was changed to chances, §9) |
| M12 clippy `--all-targets -D warnings`, `cargo fmt --check` | clean |
| OFM `cargo test --workspace` | 4,435 passed, 0 failed |
| OFM `cargo test --features mcp --lib` | 630 passed, 0 failed |
| OFM clippy default and `--features mcp`, `--locked --workspace --all-targets -D warnings`; `cargo fmt --check` | clean |
| `npm test` | 329 files, 2,908 passed, 4 skipped (pre-existing), 0 failed |
| `npm run build`, `lint`, `knip`, `format:check`, `tsc --noEmit` | clean (knip prints configuration hints only) |
| i18n gates (`vitest run src/i18n`) | pass; 1 key in 12 locales |
| Real interactive match, application level | start, orders, substitution, save mid-match, reload into a fresh state, resume, finish once, next save clean (`application::interactive_match`) |

**Reviewers.** The five read-only reviewers named in `CLAUDE.md` (architecture, i18n, accessibility, dedup,
tests) were **not run** in this session; the i18n gate and the tests above stand in for the i18n one, and the
only UI change is a route-state flag and a one-time message through the existing error dialog. They should
be run before a merge.

**Regression-effectiveness (mutation) checks.** Done for the mentality structural tests (zeroing the term
fails five of eight). Not done for the persistence, wear and finalisation tests: each was written to fail
for a stated reason first (the woodwork and own-goal tests failed with the production error before the fix;
the period-end test reproduced the production `Transition` error), but I did not remove the fix in a
disposable copy for the rest.

**Manual playability.** The desktop app was **not launched**; nothing in this report is a visual validation.
The nearest checks are the application-level tests in §7 and the frontend tests. Manual test:

1. `npm run tauri dev`, start or load a career, open the next fixture, **Manage the match**, press Play.
2. At half-time change formation and mentality, make a substitution, press Play. The second half should kick
   off in the new shape.
3. Play to about minute 30, press **Save** (or Exit to menu). Quit the app.
4. Reopen, load that save: you should land in the match at minute 30 with your tactics and substitution.
   Play to full time and **Finish**; the table moves once.
5. Save and reload again: the fixture is completed and there is no match to resume.
6. To see the discard path, close the app, change one character of the `payload` column of
   `interactive_match_checkpoint` in the save's `.db` file, load: the career opens with one notice and the
   fixture is still scheduled.

## 12. Remaining limitations

1. **Exposure does not become chances against** (§3.3): the main football gap. Needs an attack-side change.
2. Protection and ten-man football are weaker than football suggests: a defensive mentality gives no
   measured protection in goals against, protecting a lead by dropping the press made the lead *less* safe
   (scenario 1), and a red card costs about 10 to 15 % of chances and no measurable points (scenario 3).
3. The AI policies share urgency only (§6).
4. The fatigue reference was calibrated on synthetic adapted players; re-measure on FM24 safe copies.
5. Scenario 6 (a season) and a real-app playthrough were not done.
6. An own goal shows in the timeline without a scorer and is not in the scorers list.
7. Suspension and injury carry-over from an interactive match rely on the shared finalisation and were not
   re-tested with a forced card.
8. A build that changes M12 football invalidates saved in-progress matches (they restart with a notice).
9. The pin names a private repository: CI needs access configured.
10. 300 paired seeds are evidence alongside structural tests and judgement, not proof of realism; points
    cannot resolve effects below about 0.2.
11. Translations of the new notice were written for this run without a native review.
12. `FormationParams::m12()` values are my choice under the stated criterion; they have not been tuned on
    outcomes.

## 13. Git

| Repository | Branch | Base | Head |
|---|---|---|---|
| OFM | `claude/run-f4-1-football-realism` | `a69f48fb` (F4) | `6b2ddd2f` |
| M12 | `f4.1/football-realism` | `c17d5f9` (F4) | `4057c67` |

OFM, oldest first: `e82f6f7a` finalisation fixes and mid-match save; `0291efdd` hybrid fatigue; `bc270c6c`
db test fixtures for v064; `b4a6a612` shared urgency; `02d71534` Git pin; `e715946f` record tests; `cdba93dd`
docs; `6fad7be3`, `6b2ddd2f` re-pins. The report itself is committed after these.

M12, oldest first: `73807f4` mentality planner, restart geometry, probes, studies; `aa87db4` fatigue
example; `22d6c23`, `abd909c` lint and example fixes; `9d547e1` period-end fix; `9fe0439`, `0b4855a`
scenarios; `4057c67` closed-switch test. The OFM pin names `4057c67`.

The viewer repository was not touched. Not committed anywhere: the M12 licence edits that were already in the
working tree.

## 14. Push status

Both branches are pushed (OFM to `origin`, M12 to its private `origin`; M12 had no remote when F4 was
written, it has one now). Nothing was merged into `main`, `develop`, or an integration branch. CI cannot build
the OFM branch until it has read access to the private M12 repository (§8.5).
