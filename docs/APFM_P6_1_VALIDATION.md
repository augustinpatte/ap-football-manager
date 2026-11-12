# APFM P6.1: validation

Evidence and its limits for [`APFM_P6_1_TACTICAL_ALIGNMENT.md`](APFM_P6_1_TACTICAL_ALIGNMENT.md). Branch `claude/p6-1-tactical-alignment`, base
`78715a44` (P6); nothing pushed or merged. Host: macOS, Apple Silicon, rustc 1.95.0. Labels: **RUN**, **READ**, **NOT RUN**.

## 1. Verdict

**CONDITIONAL GO.**

What is verified, each with a test or a measurement that fails without it:
- **Roles.** 14 role/group pairs that never changed a match now act, through one derived rule (a holder outside the drawn group joins its pool
  with one ticket); every role's effect at equal players is between 0 and +0.047 points a match; AttackingFB is a real trade-off (+0.032 goals
  for, +0.033 against); nothing dominates (best stack +0.058 ± 0.019 against P6's +0.08 to +0.11 for dials); on real squads goals move
  3.179 → 3.215 (+1.1 %). A weight of 0 reproduces the P6 engine digit for digit.
- **Diagnosis of the AI's formation choice.** The squad model's score correlates −0.14 ± 0.03 with results across a club's own formations
  (P6's "nil" was underpowered), because its variation is a level per shape; picking its favourite earns less than a random shape
  (−0.044 ± 0.011 on fresh worlds) and sends 120 of 192 clubs to 4-3-3.
- **An evaluator that predicts better** (+0.18 correlation, +0.039 points a match over a random shape on fresh worlds; +0.18 / +0.07 on the
  shipped world), shape-neutral by construction, measured and carried on every `FormationScore`.
- **No regression:** the AI's formation behaviour is bit-for-bit P6's (three ten-season careers, same tables to the cell), invariants clean,
  cost flat (+1.3 % instructions, −1.5 % memory), workspace green.

Why not GO:
1. **The headline AI improvement is not shipped.** The better ranking raises the league's goals by 15 to 20 % because the five-zone engine
   reads a group by its mean (section 5); the AI still sends most clubs to 4-3-3. It needs an engine change this phase was told not to make.
2. **The five-zone engine and M12 are no closer.** The tactical dials differ by 6 to 8 times in size and in three cases in direction; the one
   adapter change tried (style → M12 inputs) did not meet the evidence bar and was reverted.
3. **No person has played the GUI.**
4. The project's reviewer agents were not run; the evaluator was not tested on the imported FM24 world; M12's reading of roles was not measured.

Why not NO-GO: no regression found; every shipped change has a red-then-green test; the negative results are measured and stated.

## 2. Gates (RUN, final tree `d51e979c`)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, and with `--features mcp` | clean |
| `cargo test --locked --workspace --no-fail-fast` (debug) | **152 test results (143 binaries, 9 doc-test runs): 5,013 passed, 0 failed, 121 ignored**, 0 warnings; 664 s, 3.3 GB peak including the compiler (P6: 147 targets, 4,992 / 0 / 115) |
| `cargo test --locked --lib --features mcp` | **667 passed**, 0 failed, 10 ignored (P6: 667) |
| `ofm_core` formation tests | 26 passed (`formation_fit` 9, `ai_formation` 17) |
| `engine` role tests | `p61_roles`: 5 passed, 2 ignored studies; `shared::phase_modifier_tests`: 15 passed |

The 21 added tests are 5 + 5 (roles), 9 (`formation_fit`) and 2 (`ai_formation`), minus the one adapter test that went with the reverted change.
The five new test binaries are `p61_roles` (engine) and `p61_world_roles`, `p61_formation_selection`, `p61_formation_stability`,
`p61_dials_both_engines` (season-harness; their studies are `#[ignore]`d). **NOT RUN:** `npm` lint, format, build and tests (**no frontend file was
touched**, no locale file modified, no string added), `/preflight` as one invocation, `cargo deny`, `cargo machete`, `npm run knip`, the project's
reviewer agents, M12's own repository tests.

## 3. Regression tests and mutations

Each substantive change has a named test; the change was removed in a disposable edit, the test went red, and the file was restored and
compared byte for byte.

| Change | Killed by |
| --- | --- |
| Attack support applied in the attacking-third contest | `an_attacking_full_back_overlaps_and_leaves_space`, `stacking_every_support_role_does_not_make_a_side_dominant`, `a_role_nobody_consulted_acts_at_weight_one_and_is_inert_at_weight_zero` (3 red) |
| Support credits a bonus only (a penalty is not credited) | `a_role_that_already_worked_is_bit_for_bit_the_same_with_support`, `support_credits_an_unconsulted_bonus_and_nothing_else` |
| A role's cost charged with its benefit | `an_attacking_full_back_overlaps_and_leaves_space`, `a_roles_cost_is_charged_only_with_its_benefit` |
| A holder's share diluted by the pool (one ticket) | `a_holders_share_is_his_bonus_over_the_pool_he_joins` (**the behavioural tests pass either way**; only the unit test kills it) |
| A weight of 0 is the P6 engine; an older configuration reads 0 | `an_older_configuration_reads_a_weight_of_zero`; and the P6 study reproduces digit for digit (below) |
| Evaluator is shape-neutral | `a_typical_squad_is_worth_the_same_in_every_shape` (red when a price for 3-5-2 is added) |
| Evaluator reads attributes, not the stored overall; weight of finishing; weight of the back line | `the_stored_overall_is_not_what_the_engine_reads`, `a_side_that_finishes_better_...`, `a_side_whose_back_line_holds_better_...` (mutation checked on the first version of the code: red with the overall read instead) |
| Squad at full strength, not who can play today | `a_squads_shape_does_not_move_with_this_weeks_injuries` (red before the change) |
| The AI's total is `squad + taste`, not the fit | `a_formations_total_is_the_squad_models_score_and_the_taste_and_not_the_engines_fit` |

**Not mutation-checked:** the committed values of the typical levels and the two weights (measured, not unit-tested: no test fails if they
change), `role_support_weight = 1.0` as the shipped default (measured; the unit tests state the behaviour, not the value), the stacking cap
(`ROLE_SUPPORT_LIMIT`, which the shipped measurements never reach).

## 4. Determinism and baseline reproduction (RUN)

| Check | Result |
| --- | --- |
| P6 baseline study (`p6_what_predicts_a_formation`, 24 clubs), P6's tree vs this tree at weight 0 | **identical digit for digit** (slope −0.0142 ± 0.0294, spread 0.713, range 2.182) |
| The formation study run twice, same command | identical to every digit (all 14 candidates) |
| Three ten-season gate careers with the final code against the same run with P6's formation rule and the roles on | **identical tables to the cell** (formations, top share, changes, finance, squads, loans), invariants clean ×3 |
| Role audit at weight 0 | 0.0 % of 20,000 matches changed for each of the 14 pairs |

## 5. Long careers and league-level effects (RUN)

| Run | Result |
| --- | --- |
| 3 seeds × 10 seasons, generated gate world (80 clubs), invariants on, final tree | invariants **clean ×3**; 5.9 formations in use, top share 46.7 %, 23 formation changes a season; about 45 s a ten-season career with the invariants off |
| Six real worlds, same seeds, 6,840 matches each, warmed 60 days | **P6 engine (roles off): 3.179 goals, 25.68 shots, 7.72 on target, 21.8 % draws, 1.561 home points. P6.1 as shipped: 3.215 goals, 25.94 shots, 7.79 on target, 21.4 % draws, 1.542 home points** |
| The same worlds with the AI ranking by the engine fit (not shipped) | 3.649 goals (with a price per shape) / 3.810 (shape-neutral); on target 8.19 / 8.44; draws 18.9 / 19.6 %; shots 25.6 flat |
| Every league club forced into one shape (120 clubs, 6 worlds) | goals a match 3-4-3 3.13, 4-3-3 3.19, 3-5-2 3.35, 4-1-4-1 3.35, 4-5-1 3.43, 4-2-3-1 3.45, 5-3-2 3.45, 4-4-2 3.51 |
| AI formation rule sweep, engine fit with shape prices (three careers each, **not shipped**) | changes a season: margin 0.25 / 0.5 / 1.0 / 1.5 → 110 (players who can play today) → 67 (whole squad) / 54 / 34 / 26; P6 rule: 23 |

## 6. Cost (RUN, bare release test binary under `/usr/bin/time -l`, invariants off, machine at load 3)

| Run | P6 | P6.1 | Change |
| --- | --- | --- | --- |
| Gate, 3 seeds × 10 seasons | 2,533.1 B instructions, 186.9 MB (peak footprint 165 MB), 135-136 s | **2,566.0 B**, **184.1 MB** (peak footprint 165.9 MB), 132 s | **+1.3 %**, −1.5 %, flat |

Instructions retired are load-independent and are the figure to trust; wall time is not a result on a shared machine. The costs added are a scan
of eleven players a contest for each of three support phases (`role_support`) and, once a review (about every twelve weeks a club), eight selections of an eleven for the
evaluator (`formation_fit`); the review's cost was not measured separately, and the +1.3 % covers both.

## 7. Studies (RUN)

| Study | Scale | Reported |
| --- | --- | --- |
| `engine/tests/p61_roles.rs::p61_role_audit` | every role and group × 20,000 matches, weight 0 and 1, all holders and two, gap −6/0/+6 | alignment 3.2-3.5 |
| `engine/tests/p61_roles.rs::p61_points_per_rating_point` | +1 to +5 rating points × 40,000 matches | alignment 4.4 (0.136 goals) |
| `season-harness/tests/p61_world_roles.rs` | 6 worlds × 1,140 matches; census of 120 squads; forced-shape worlds | alignment 3.4, 4.5 |
| `season-harness/tests/p61_formation_selection.rs` | 192 clubs × 8 formations × 96 matches (147,000 matches), leave one world out; frozen on 192 fresh and 48 shipped-world clubs | alignment 4.1-4.4 |
| `season-harness/tests/p61_formation_stability.rs` | 3 worlds × 17 weeks of weekly reviews, formations held | alignment 4.5 |
| `season-harness/tests/p61_dials_both_engines.rs` | 12 settings × (3,000 five-zone + 400 M12) matches, paired seeds | alignment 5.2, 5.3 |
| `p6_formation_contests` (P6's) | shape matrix 20,000 matches a cell; weight fit 6,000 a shape | alignment 5.1 |
| Career census, final tree and variants | 3 seeds × 10 seasons × 6 variants | alignment 4.5 |

**Independent units.** Clubs (192 over 12 gate worlds; 192 over 12 fresh ones; 48 over 3 shipped-size worlds) for the formation studies, with leave-one-world-
out so no club is scored by a rule fitted on its own world; worlds (3 or 6) for careers and league rates; **matches for the equal-player studies**,
where the standard error is of matches and player quality is identical by construction. **The M12 comparison is one club pair, one seed, 400
matches a setting** (points se 0.09, shots 0.24).

## 8. Limits of this evidence

1. The evaluator's cross-validated figures (+0.45, gain +0.11) are **not** its out-of-sample figures (+0.18, +0.039; the typical levels and the
   weights were fitted on the worlds the cross-validation scored). The latter are the ones to quote. It was fitted on generated gate worlds, tested on
   the shipped 440-club world (48 clubs, wide error), **not on the imported FM24 world**.
2. The goal-rate finding is from six worlds and 6,840 matches each under the five-zone engine; **M12's goal rate under a different AI was not
   measured**. The mechanism (a group read by its mean) is read from the code and supported by the forced-shape table, not isolated by an experiment
   that changes the reading.
3. M12 numbers are P6's (formations: 500 matches a shape, se about 0.06) and this phase's (dials: 400 matches, one club pair). A 0.05 effect is
   invisible there; the very low line (−0.388, 4.2 standard errors) and the pressing and defensive-style shot effects are the only ones resolved.
4. The role audit is at equal players, 4-4-2 against 4-4-2, one holder group at a time. Roles in combination with traits, styles and the other
   dials, and against other shapes, were measured only for stacking.
5. **No GUI.** Nothing was launched, clicked or watched.
6. The legacy `engine::simulate` (the one-shot engine, `sim-bench` only) was not changed and still differs from the live engine.
7. Not one of M12's own tests was run, and M12 was not modified.
8. The project's reviewer agents were not run (the phase was not opened as a pull request): a reviewer pass is owed before one is.

## 9. Incidents during the phase (RUN, recorded as they happened)

- **macOS paste protection.** XProtect blocked a paste into the VS Code terminal four times on 12 Nov (09:13, 10:15, 10:16 twice), each tied to the
  clipboard service with `path=(none)`. No file or script was involved: none created by this session was quarantined, there are no hooks, and the
  clipboard held the text of the instruction pasted. Nothing was disabled or bypassed. No further warning has been reported to this session since approval to resume.
- **Scratchpad reset.** The session's scratch directory was emptied on resume (the machine had also restarted); a census whose output could not
  be written reported success and a polling loop waited on a file that could not appear. Diagnosed, the loop stopped, nothing lost that was not
  recorded elsewhere; later runs checked the file existed.
- **Two zero-test selections** (a `|` filter and a second filter before `--` in `cargo test`): neither was taken as evidence; the studies were rerun with
  `running N tests` confirmed.
- **A nested copy-on-write clone** of a build directory (4.2 GB, inside this worktree's own `target/`) from `cp -cR` into an existing directory: noticed,
  checked, removed. No other worktree's files were modified; its build artifacts were only read.
- A first explanation for the goal-rate rise (a herd into back threes) was tested and **wrong**; the forced-shape experiment replaced it.

## 10. Reproduce

```sh
# from src-tauri/, release; every command is deterministic
P61_MATCHES=20000 cargo test --release -p engine --test p61_roles p61_role_audit -- --ignored --nocapture        # also P61_HOLDERS=2, P61_SUPPORT=0|1, P61_GAP=-6|0|6, P61_ROLES=A,B
P61_MATCHES=40000 cargo test --release -p engine --test p61_roles p61_points_per_rating_point -- --ignored --nocapture
P6_SEEDS=1,2,3 cargo test --release -p season-harness --test p6_cross_engine -- p6_what_predicts p6_formation_score --ignored --nocapture --test-threads=1
P6_SEEDS=1,2,3,4,5,6,7,8,9,10,11,12 P61_CLUBS=16 P61_REPS=8 cargo test --release -p season-harness --test p61_formation_selection -- --ignored --nocapture --test-threads=1   # P6_SEEDS=13..24 and P6_WORLD=shipped for the frozen rows; P61_DUMP=<tsv>
P6_SEEDS=1,2,3,4,5,6 P61_REPS=3 cargo test --release -p season-harness --test p61_world_roles -- --ignored --nocapture --test-threads=1                         # P61_FORCE=4-4-2, P61_NOROLES=1
P6_SEEDS=1,2,3 cargo test --release -p season-harness --test p61_formation_stability -- --ignored --nocapture
P61_SETTINGS=Possession,VeryLow P61_MATCHES=3000 P61_M12_MATCHES=400 P6_SEEDS=1 cargo test --release -p season-harness --test p61_dials_both_engines -- --ignored --nocapture   # run four processes of three settings
P6_MATCHES=20000 cargo test --release -p engine --test p6_formation_contests p6_equal_players_shape_matrix -- --ignored --nocapture
P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 cargo test --release -p season-harness --test p45_long_career p45_long_career -- --ignored --nocapture --test-threads=1    # P45_INVARIANTS=0 and the bare binary under /usr/bin/time -l for cost
```

Note for `cargo test`: **one** filter goes before `--`; a second filter goes after it with `--ignored`. A selection of zero tests exits 0.

## 11. Manual GUI checklist (for the user; none of it is claimed)

1. Give a defender the **AttackingFB** or **WingBack** role and a midfielder **ShadowStriker**; quick-simulate a few matches against the same
   opponent with and without: the side should score and concede a little more. The role descriptions are unchanged.
2. Watch an AI club's formation over a season: it should behave as in P6 (a few changes a season across the league, most clubs in 4-3-3).
3. In a watched match (M12) set the **Possession** style and the **High Press** style: the style still reaches M12 as mentality alone
   (alignment 5.3); do not expect a visible difference in passing or pressing.
4. Set a **very low defensive line**: in M12 it costs a visible amount of points and gives the opponent fewer shots; in a quick-simulated match it
   is nearly free (alignment 5.2).
