# APFM Run F4.4: structural football correction, calibration and career validation

Branches: OFM `claude/run-f4-4-structural-validation` (from the F4.3 head `2243f826`), M12 `f4.4/structural-validation` (from the F4.3 head
`dc6db5ea94b9862f32f4364a978df2c070d588f5`). Both heads were verified before anything was changed. Nothing was merged into `apfm/main`,
`develop` or an integration branch. No save and no existing career was touched (every study builds a generated world in memory), the FM24
data was not opened, and the 2D viewer was not modified. The M12 licence edits that were already uncommitted (`Cargo.toml`, untracked
`LICENSE` / `NOTICE`) are in no commit.

## 1. Executive verdict

**NO-GO. This run did not complete the mission, and F5 should not treat the football as settled.** It did the first checkpoint properly
(reproduce, isolate, classify the scoring explosion) and fixed two of its causes. Most of the other priorities were **not started**; that
is stated per item in section 22 rather than implied by silence.

What is established, with evidence:

* **The F4.3 hypothesis for the scoring explosion was wrong.** It was not quality-gap amplification. The user's club in the studied worlds
  is *not* the strongest (outfield rating rank 57 and 67 of 80), world gaps are small (best 71.8, median 68.9, worst 66.5), and the engine
  already produced **5.7 goals a match at a gap of 0 to 2** (strong side 2.88, weak side 2.86).
* **F4.3's baseline of "about 2.5 goals at equal level" counted open-play goals only.** Total goals of the same synthetic 65 v 65 fixture
  are **3.3** (penalties and set pieces included), which is what a career score counts. The honest engine-versus-career excess is 3.3 to
  4.8 on real inputs, not 2.5 to 5.4. (Class G, test-harness definition.)
* **Two real defects were located and the first was fixed in OFM, the second in M12.** Section 5.
* **After both fixes the career still scores too much** (one seed: 4.71 a match against 2.83 for the rest of the world, down from 5.42).
  Gate A therefore remains failed.

| Gate | Status | One line |
|---|---|---|
| A. Scoring | **Not met** | 5.42 to 4.71 goals a match in the user's club's matches (one seed, 38 matches); world 2.83 |
| B. Movement | **Not started** | no movement work was done; F4.3's 17 km midfielders stand |
| C. Shots | **Not started** | shot volume fell 52 to 42 with the geometry fix, but no distribution work was done |
| D. Numerical strength | **Not started** | the staged regression of F4.3 was not investigated |
| E. Tactics | **Partly investigated, not met** | a defensive-line cliff was found and fixed; a high line still costs about +0.9 goals |
| F. Discipline | **Not started** | |
| G. Career | **Not met** | one season re-measured; no multi-season validation; dismissals not investigated |
| H. Technical reliability | **Met for what changed** | M12 suite 625 passed, 0 failed; OFM spatial and interactive suites pass; clippy and fmt clean; mutation check on the seam |
| I. Playability | **Not met** | no GUI session was possible or attempted; checklist in section 19 |

## 2. F4.3 baseline (reproduced)

Seed 7, generated world, release build, F4.3 engine (`dc6db5e`), the user's club played every fixture as an interactive match:
**5.42 goals a match in the club's 38 engine matches, 2.84 over the 1700 completed fixtures of the whole world, 12 red cards.** This
reproduces the F4.3 report's 4.7 to 8.0 range. Raw output: the `debug_one_season` study of `interactive_season.rs`.

## 3. Root causes, classified

| # | Finding | Class | Evidence |
|---|---|---|---|
| 1 | "Career goals exploded because quality gaps are amplified" | **H/G: wrong hypothesis** | gap 0-2: strong 2.88, weak 2.86, total 5.73 (n=49 engine matches); gap 2-4: 3.25 v 2.31 (n=16) |
| 2 | "2.5 goals at equal level" | **G: definition** | synthetic 65 v 65, total goals 3.33 (180 matches); F4.3's 2.45 was open-play |
| 3 | OFM 4-4-2 laid out with strikers on the touchlines (y = 8 and 60) and every line 4 to 10 m shallower than M12's reference | **C: OFM-to-M12 mapping** | flat teams, neutral tactics and instructions: 5.92 goals, 72 shots; same with the reference geometry: **3.33**, 37 shots (= the synthetic control) |
| 4 | A defensive-line cliff: the slider is binned and the High bin moves the whole line +14 m in one step | **A: engine logic** | line 60: 4.34 goals; line 66: 5.12 (480 matches each), shots unchanged |
| 5 | Even with 3 and 4 fixed, a high line costs goals | **A: engine logic, unresolved** | line 85 against 60: +0.9 goals, +2.6 offsides, +0.2 penalties, shots unchanged (40.5 v 41.3); goals within 10 s of a turnover only +0.34 of the +0.9 |
| 6 | Real attribute heterogeneity (specialised players) | **D: world generation / B** | +0.25 goals (3.33 to 3.58) |
| 7 | OFM presets map to extreme tactical values (HighPress is pressing 99, line 85; 66 of 120 sides play line 85, mean tempo 61) | **E/C** | neutralising only the line height takes 4.77 to 3.82 goals; neutralising press, tempo, width, directness each changes goals by under 0.3 |
| 8 | Out-of-position XIs in generated worlds (an LB at RB, a ST in a CM slot, a DM at LM) | **D/F** | observed in the dumped lineups; effect **not measured** |
| 9 | Condition, keeper attributes, role instructions | ruled out as main causes | no fatigue 5.11 v base 4.79; keeper set to 65 4.96; neutral instructions with real attributes 3.58 v 3.33 |

### What did not matter, measured

Fatigue (starting condition), the goalkeeper mapping (reflexes and handling taken from OFM, `one_on_one` and `kicking` fixed at 50), flat
versus real attribute spread beyond +0.25, and the five other tactical sliders.

## 4. Changes by subsystem

| Area | Change | Files |
|---|---|---|
| OFM adapter | 4-4-2 and every formation: lines at x = 26 / 48 / 70 (four-line formations 26 / 40 / 54 / 70), a line of *n* players spaced evenly about the centre at most 16 m apart (a pair of strikers at y = 26 and 42, a back four at 10, 26, 42, 58) | `spatial_adapter.rs` (`formation_geometry`, `spread_y`) |
| M12 formation | `continuous_line`: the defensive line follows `line_height` by interpolation between the centres of the old bins (17, 50, 83) | `formation.rs` (`line_block_terms`, `FormationParams`) |
| M12 config | `M12Config::continuous_line`, `FootballConfig::full_match_f44()` (= F4.3 + continuous line), flags `f44`, `continuousline` | `config.rs`, `export.rs` |
| OFM | plays `full_match_f44` (one function, both call sites) | `spatial_runner.rs` |
| Instruments | M12 `examples/f44_career_inputs.rs`; OFM `scoring_response_by_quality_gap` and `dump_adapted_career_inputs` (ignored studies) | see section 21 |

Everything is behind a switch that is off in `full_match_m12/f42/f43`; **the whole M12 suite, golden digests included, passes unchanged.**
`BEHAVIOUR_REVISION` is unchanged for that reason.

### Measured and not adopted (removed from the code)

* **A line that drops while nobody of its own is on the ball** (hold radius 8 m, release 12 m, 3 s dwell): line 85 stays at 5.33 goals
  (against 5.12 without it). The continuity fix alone accounted for what improved at line 66 (4.67).
* **A keeper who steps out with the line** (0.4 and 0.8 of the line's advance): line 85 5.30 and 5.08 against 5.22, inside the noise.
  The keeper was the obvious suspect behind a line 35 m from him and it is not the cause.

## 5. Career scoring analysis

Instrument: real OFM-adapted inputs of 60 sampled fixtures of a generated world (seed 7), run on M12 `full_match_f43` with edits to the
JSON input only, 8 seeds each (n = 480 matches a variant, 95 % intervals about +-0.2 goals).

| variant | goals | shots | on target |
|---|---:|---:|---:|
| base (as OFM built it) | 4.79 | 52.3 | 26.2 |
| no fatigue | 5.11 | 54.1 | 27.4 |
| neutral tactics | 5.59 | 64.4 | 30.4 |
| flat teams (per-player attributes equalised) | 4.24 | 48.8 | 22.6 |
| goalkeeper set to 65 | 4.96 | 52.3 | 26.5 |
| synthetic identical teams, same mean rating | 3.35 | 37.8 | 16.8 |
| flat + neutral tactics + neutral instructions | 5.92 | 71.9 | 31.0 |
| the same with the reference geometry | **3.33** | 37.2 | 15.4 |
| base with the reference geometry | 4.77 | 43.1 | 19.8 |
| base + reference geometry + neutral tactics | 3.78 | 38.6 | 17.1 |
| base + reference geometry + neutral tactics + neutral instructions | 3.58 | 38.2 | 17.2 |

After the OFM geometry fix (inputs re-dumped from the fixed adapter): base **5.05 goals, 42.4 shots** (shots fell 52 to 42; goals did not, the
high line carries them). Neutral tactics 4.16.

Identical synthetic teams by level (180 matches each): level 50 2.86, 55 2.88, 60 3.12, 65 3.33, 70 3.42, 75 3.39, 80 3.51 goals. **The engine's
own level response is mild**, which is another reason the explosion is not a rating effect. Note that even the engine's neutral fixture
(3.3 to 3.5 goals) is above real football (the OFM statistical model gives 2.84 over this world; real leagues about 2.7 to 3.0): the engine
base rate is itself too high, and F4.3's shot volume (37.9 here against about 27.6 in the chosen reference) is the same finding.

Quality-gap study (the user's club, three seeds; two seasons completed, seed 11 ended with a dismissal after 33 matches), goals by the gap
between the two clubs' outfield ratings:

| source | gap | n | stronger side | weaker side | total |
|---|---|---:|---:|---:|---:|
| engine (the user's club) | 0-2 | 49 | 2.88 | 2.86 | 5.73 |
| engine (the user's club) | 2-4 | 16 | 3.25 | 2.31 | 5.56 |
| statistical model (others) | 0-2 | 3556 | 1.42 | 1.38 | 2.80 |
| statistical model (others) | 2-4 | 1214 | 1.46 | 1.38 | 2.84 |
| statistical model (others) | 4-7 | 105 | 1.68 | 1.18 | 2.86 |

No gap of 4 or more occurred in an engine match, so **large-gap response in the career was not measured**. The synthetic gap ladder of F4.3
(72 v 58: 3.15 open-play goals, 75 v 50: 4.38) was not re-run.

After the two fixes, seed 7, same season: **4.71 goals a match** in the club's engine matches (5.42 before), 2.83 for the world, 7 reds (12
before). One seed, 38 matches, standard error about 0.4: this is a direction, not a result.

## 6 to 12. Quality-gap response, movement, shots, numerical strength, pressing, mentalities, discipline

**Not done in this run.** Nothing in F4.3's findings on these was re-measured or corrected, and none of F4.3's numbers should be read as
changed. The shot-volume improvement noted in section 5 is a side effect of the geometry fix and was not examined as a distribution.

The one relevant finding is the **defensive-line response** (section 3, rows 4, 5, 7), which belongs to the tactics gate:

| line height (both teams, base inputs, reference geometry, 480 matches) | goals | shots | offsides | penalties | goals within 10 s of a turnover |
|---|---:|---:|---:|---:|---:|
| 10 | 5.36 | 79.6 | | | |
| 35 | 4.01 | 44.1 | | | |
| 50 | 3.82 | 42.6 | | | |
| 60 | 3.89 | 42.3 | 1.9 | 0.51 | 0.37 of 4.19 |
| 85 | 5.22 | 41.2 | 4.6 | 0.72 | 0.71 of 5.14 |
| 100 | 5.30 | 41.4 | | | |

(Rows 10 to 50 and 100 were measured before the continuous line; the continuous line leaves 50 and 60 unchanged and smooths 61 to 83.)
A very low line gives 80 shots; a high one the same shots but better chances. **Real high-line sides (about 40 m from their own goal) do not
concede a goal more a match.** The cause is in the passing and defensive-recovery logic and is open.

## 13. Career integration

Not done: the M12 and OFM-statistical comparison is only the two goal columns above, which show the statistical model is flat in the gap
(2.80, 2.84, 2.86 goals) while its stronger side's win share rises 39, 39, 47 %.

## 14. Board-dismissal findings

Not investigated. Observed again: seed 11 of the gap study ended with a dismissal after 33 matches (26 league matches, 22 points); seed 7
and 23 completed 38 matches.

## 15. Multi-season validation

Not done. One 38-match season was replayed on seed 7 and reproduced F4.3's invariants (the study asserts them); that is **one** season.

## 16. Technical regression tests

| Check | Result |
|---|---|
| M12 `cargo test --release` (all targets) | **625 passed, 0 failed, 29 ignored** (studies); includes the historical golden digests of Missions 8 to 12 and the pinned F4.3 digest |
| M12 `cargo clippy --release --all-targets -- -D warnings`, `cargo fmt` | clean |
| New M12 tests (`tests/f44_line.rs`) | 4 pass: the binned rule jumps the line by over 10 m (the defect, pinned); one point of the slider moves the line under 1 m; monotone; bin centres keep their positions |
| OFM `cargo test --release -p ofm_core --lib -- spatial interactive_match` | **98 passed, 0 failed, 2 ignored** |
| New OFM tests | geometry (failed first with `striker at y=8`, as predicted); instant path plays F4.4; interactive session and its restored checkpoint play F4.4; the configuration function is F4.4 |
| **Mutation check of the seam** | `match_football_config` reverted to F4.3: **all three call-site tests fail** (instant, interactive, configuration); restored |
| OFM clippy `--workspace --all-targets --locked -D warnings` (from the repo root, i.e. against the *pinned* engine revision), `cargo fmt --check` | clean |
| Pin | `ofm_core/Cargo.toml` and `Cargo.lock` pin M12 `d4e6f329d7e48279a9dce5f7f3c1b970f7ff2131`, which is pushed; `cargo check --locked -p ofm_core -p db` resolves it with the local override moved aside |
| Frontend, MCP tests, build, full OFM workspace tests, end-to-end | **not run**: no frontend or MCP code changed. This is a gap, not a pass |

The F4.3 reviewers' coverage gap (either call site silently reverting to F4.2) is closed for both call sites by the tests above.

## 17. Performance

Not measured separately. The change adds one interpolation per target computation; the 600-match suites ran at the F4.3 pace.

## 18. Reviewer findings

See section 23 (filled in from the three reviewers that apply: architecture, test, dedup; the i18n and UI-accessibility reviewers were not run
because no player-facing string and no frontend file changed).

## 19. Desktop validation

**Not performed.** No GUI session was attempted. The closest supported workflow that did run is the application-layer season harness (an
interactive match played through `advance_interactive_match_internal` and finalised through the one canonical path, 38 matches, seed 7).

Manual checklist for the user: create a temporary career (not a real save); pick a club; set a high press and a high line and check the
score against the world average (about 2.8); start a match; change mentality; make a substitution; provoke or wait for a red card and
confirm the suspension afterwards; save mid-match, quit, reload, finish; check the table, the suspension list and the scorers.

## 20. Remaining limitations

1. The career engine still scores about 4.7 goals a match (one seed). The engine's neutral base rate (3.3 to 3.5 total goals, 37 to 38 shots)
   is above football's, and a high line adds about 0.9 goals through settled-possession chances behind it.
2. OFM presets map to extreme tactical values (pressing 99, line 85). Whether that is a mapping defect or the preset's intent is a product
   decision; the engine response to it is the defect.
3. Out-of-position XIs in generated worlds are observed and not measured.
4. Everything in sections 6 to 15 above is undone.
5. The statistical model and M12 were not compared beyond two columns.

## 21. Reproduce

```bash
# OFM, from src-tauri/ (uses the local M12 checkout through the git-ignored .cargo override)
cargo test --release --lib debug_one_season -- --ignored --nocapture                    # F43_SEASON_SEED=7
F44_SEEDS=7,11,23 cargo test --release --lib scoring_response_by_quality_gap -- --ignored --nocapture
F44_DUMP_DIR=/tmp/f44/inputs7 F44_SEED=7 F44_SAMPLE=60 cargo test --release --lib dump_adapted_career_inputs -- --ignored --nocapture
# M12
cargo run --release --example f44_career_inputs -- /tmp/f44/inputs7 8 base nofatigue neutral_tactics flat_teams plain_gk synth
cargo run --release --example f44_career_inputs -- /tmp/f44/inputs7 8 base+synth_geometry+line=60 base+synth_geometry+line=85
F44_FLAGS="f44" cargo run --release --example f44_career_inputs -- /tmp/f44/inputs7g 8 base+line=66
```

Variants compose with `+`: `nofatigue neutral_tactics neutral_instr flat_teams plain_gk synth_geometry central_strikers n_line n_press
n_tempo n_ment n_width n_direct line=<v> synth synth:<level> no_plan`. Raw outputs were printed to the session and the tables above are
copied from them; the instruments regenerate them.

## 22. Mission items, one by one

| Item | Status |
|---|---|
| 3. Reproduce and classify | **Done** (sections 2, 3) |
| 4. Career scoring explosion | **Partly**: two causes found and fixed; the residual is open |
| 5. Movement model | **Not started** |
| 6. Shot generation and chance quality | **Not started** (shot volume fell as a side effect only) |
| 7. Numerical superiority | **Not started** |
| 8. Pressing and technical resistance | **Not started** |
| 9. Mentality trade-offs | **Not started** (a defensive-line finding only) |
| 10. Fouls, cards, penalties | **Not started** |
| 11. Career engine consistency | **Not started** |
| 12. Board dismissals, multi-season | **Not started** |
| 14. Regression and integration tests | **Done for what changed**; frontend/MCP/e2e not run |
| 15. Desktop validation | **Not performed** |

## 23. Git, push status, reviewers

* M12 `f4.4/structural-validation`: `d4e6f329d7e48279a9dce5f7f3c1b970f7ff2131` (pushed to `origin`).
* OFM `claude/run-f4-4-structural-validation`: the commit that carries this report (see `git log`); **push status is recorded in the commit
  message trail and the final message of the run**.
* **CI secret:** `gh secret list --repo augustinpatte/ap-football-manager` returns **no secrets**, so `APFM_ENGINE_READ_TOKEN` does not exist
  and the private-engine CI is not operational. Required action for the owner: create a read-only token for `augustinpatte/apfm-match-lab`
  and store it as the repository secret `APFM_ENGINE_READ_TOKEN`.
* **Reviewers run:** `ofm-architecture-reviewer`, `ofm-test-reviewer`, `ofm-dedup-reviewer`. **Not run:** `i18n-auditor` and
  `ui-accessibility-reviewer` (no player-facing string and no frontend file changed; stated, not skipped silently).

| Reviewer | Finding | Disposition |
|---|---|---|
| architecture, test | the `replay_under` helper orphaned an existing Given/When/Then comment | **fixed** |
| architecture | the new engine pin may not be on the remote | **checked**: `git ls-remote` shows `d4e6f32` on `origin`, and `cargo check --locked -p ofm_core -p db` resolved it with the local override moved aside |
| architecture | geometry change alters the adapted input and digest of every new match | **by design**; checkpoints store the input and the football, so an existing save replays as recorded. Not exercised against an old save in this run |
| test | geometry test held two scenarios; 3-line, 3- and 5-player cases untested | **fixed**: three tests with literal coordinates |
| test | interactive test held two scenarios; a restore cannot detect a call-site revert | **fixed** (split); the restore test says what it proves, the start test and the instant-path behavioural test carry the revert detection |
| test | `f44_line.rs`: only `a_one_point_change...` fails on the binned rule; the others are a positive control and guards | **accepted as stated**; the tests do not cover compression, possession or transition phases |
| test | the ignored studies are not tests (no assertion, no G/W/T) | **accepted**: they are instruments in an ignored module, listed in section 21 |
| dedup | `outfield_rating` / `club_strength` duplicate the idea of `ofm_core::catchup::club_strength` (not public) and `player_rating` | **renamed** `engine_input_strength` / `club_input_strength` with a comment: the study deliberately uses the engine's input, not the model's `ovr`. A third copy of club strength exists in `season-harness/examples/apfm_balance.rs`; for epic #589 |
| dedup | `x_rows` 26/48/70 duplicates M12's reference depths | **open**: M12 exposes no constant; the test pins the values |

## 24. GO / NO-GO for F5

**NO-GO.** Gates A, B, C, D, F, G and I are not met, E is not met, H is met only for what this run changed. The next session should start
from sections 3 and 20: (1) why a high line produces settled-possession chances in the engine's pass-into-space and recovery logic,
(2) the engine's neutral base rate (3.3 total goals, 38 shots), (3) then the movement model, which F4.3 already identified and this run did
not touch.
