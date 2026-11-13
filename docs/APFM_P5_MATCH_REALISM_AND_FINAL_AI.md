# APFM P5: final sporting AI and match realism

Branch `claude/apfm-playable-career`, from `4d7e7acf` (P4.7). Not merged into `apfm/main`, **not pushed** (section 14). The
external 2D viewer, the world database and the FM24 import pipeline were not touched. The M12 engine repository
(`apfm-match-lab`, rev `28f34b10`) was not modified. The evidence is in `docs/APFM_P5_VALIDATION.md`.

Evidence labels: **RUN** (executed this phase), **READ** (read in source), **REVIEW** (a reviewer agent's finding, checked here).

## 1. Verdict

**CONDITIONAL GO.** The sporting AI is **not declared complete.** Two things this phase set out to establish could not be
established, for a reason that is itself the main finding: the match engine every AI fixture is played by cannot price a
formation's fit to a squad, so a formation, a match-day shape change or a squad-model score has **no demonstrable consequence in a
result**. What was found, fixed and verified is listed below; what is left, and the one precise next step, is in section 13.

| Finding | Status |
| --- | --- |
| **The engine scored 3.9 goals a match** in every followed league (26 shots, 10.8 on target). Cause: shot accuracy was measured from a fixed 50, so the football calibrated on lower ratings scored 40 % more in every world rated since. **Fixed:** shot accuracy is relative to the side defending, base recalibrated 0.35 to 0.29 | **Fixed and verified** (2.7-3.4 goals a match across worlds, 22 % draws) |
| The squad model's score for a formation does **not** predict how the formation performs (−0.021 ± 0.021 points a game per rating point; the model's best shape is best in 12 % of clubs against 12.5 % by chance) | Established, 24 clubs × 3 worlds, 18,432 matches |
| No formation dominates (spread ±0.09 points a game); some tactical dials do (possession +0.125, a very low line +0.07) | Measured; **not fixed** |
| A match-day shape change for missing players: built, measured over twelve worlds, **no demonstrable benefit** | Built, **not enabled** |
| The saved eleven lost a slot on every departure (retirement, release, sale, loan out, loan return, loan-to-buy) | **Fixed**, mutation-checked on every path |
| Agreed deals waiting to register were not reserved against a second bid | **Fixed** (fees and wages, decision time only) |
| Manager identity did not reach the match; an injured player played on | **Fixed** (risk and adaptability reach the touchline; a hurt player is replaced) |
| Youth minutes | Investigated: **no defect**, nothing changed |
| GUI | **Not played.** No desktop control exists in this session; an executable checklist is extended |

## 2. Baseline

| | |
| --- | --- |
| HEAD at start | `4d7e7acf` (`git rev-parse HEAD`), tree clean |
| Ancestry | `6667ef60` **is an ancestor** of `4d7e7acf` (`4d7e7acf` is the documentation commit on top of it); 13 commits since `b5430826`: both hashes in the brief are right, they name the implementation and the docs |
| Remote | `origin` = `augustinpatte/ap-football-manager`, **public** (section 14) |
| M12 | `apfm-match-lab` is a separate **private** repository, pinned at `28f34b10` in `ofm_core/Cargo.toml`; a local checkout exists at `~/apfm-match-lab`, at the same rev (with an uncommitted `Cargo.toml` change and untracked `LICENSE`/`NOTICE`, left alone) |
| Baseline workspace tests (**RUN**) | `cargo test --locked --workspace --no-fail-fast`: **140 targets, 4,856 passed, 0 failed, 94 ignored**, identical to P4.7's report |

## 3. Commits (oldest first, on `4d7e7acf`)

| Commit | Change |
| --- | --- |
| `bf4a385c` | `fix(p5)`: a leaving starter's slot in the saved eleven is refilled; agreed deals reserve their fee and wage |
| `efb9e4cd` | `feat(match-ai)`: a manager's risk and adaptability reach the touchline; a player hurt in the match is replaced |
| `64e07fca` | `fix(engine)`: shot accuracy is relative to the defence, not to a fixed 50; the controlled match studies |
| `1cdfdc58` | `test(p5)`: the shape for the day, studied and measured, **not wired** into match preparation |
| `4e1b5f2f` | `test(p5)`: cross-engine match preparation, formation persistence, the reinvestment rule on real numbers |
| `e2c74619` | `fix(p5)`: act on the architecture and dedup reviewers |
| `74d95613` | `test(p5)`: act on the test reviewer |
| (this report) | `docs(p5)` |

## 4. Which engine plays what (**READ**)

| Path | Engine | Used for |
| --- | --- | --- |
| Unwatched fixtures: `turn::simulate_other_matches` → `live_match_manager::play_unwatched_fixture` | **OFM five-zone live engine** (`engine` crate, `MatchMode::Instant`, an AI manager on both touchlines) | Every AI v AI match, every delegated or spectated user match, **every long-career study** |
| User's watched or interactive match, replay, `interactive_season` | **M12** (`apfm-match-lab`, via `spatial_runner`, `interactive_match`) | The match the user watches or steps through |
| Competitions outside the followed scope (`Scope::Followed`, the default) | **The dormant tier**: a statistical scoreline model (`catchup::resolve_fixture_by_scoreline`, `turn/dormant.rs`) | Every league the user does not follow: 2.4-2.5 goals a match |
| `engine::simulate`, the one-shot "legacy quick" engine | the engine crate's original resolver | Only `sim-bench` and tests; no career match |

Differences between AI v AI, human v AI and delegated: **none in the engine**. All three build the two sides with the same
`turn::squad::build_team_with_bench_from`; the user's club alone reads its saved eleven (`select_starting_xi`) where an AI club
re-picks one for every match (`ai_select_starting_xi_by`, with the manager's rotation and youth trust), and a delegated career's
assistant writes the eleven a day before a match (`delegated_eleven`). The tactics inputs are the club's `tactics_phase` and
`play_style`, mapped by `domain_to_engine_tactics`.

**The long-career studies play the five-zone engine** (in the followed competitions) and the dormant tier (everywhere else).
Improving one engine does not improve the other: this phase changed only the five-zone engine and M12 was not touched.

M12's contract (`apfm-match-lab/INTEGRATION_CONTRACT.md`): APFM owns selection, lineup, bench, tactics and AI decisions; the
engine consumes the final team and owns in-match play. Two consequences recorded for the next phase: M12's input takes a
`formation` string and geometry from per-slot `base` positions, and the adapter reads `Team::formation` for both
(`spatial_adapter.rs`), so **any match-day shape change must be handed to the adapter as well** as to the five-zone team
(section 8); and M12 interactive changes mid-match are not in contract v1.

## 5. The finding that mattered: the goal rate (**RUN**)

Every controlled study opened with a number nobody had checked. Played through `play_unwatched_fixture`, **every** league of the
gate world scored **3.9 goals a match** (26 shots, 10.8 on target); the 2.4-2.5 of the same world's reference season came from the
dormant tier, and only the user's league (England First Division, followed) ran the real engine and scored 3.91.

What was ruled out, each by measurement: player condition (5.9 → 4.7 goals from condition 100 to 40), warm-up days and the AI's
tactics, the club's reputation (3.85 → 3.97 across 300-900), the gap in strength between the clubs (3.88 / 3.92 / 3.82 across
thirds), the league's attackers (the league with the best defenders and keepers scored most), and the path (the day's sweep and
a direct call gave identical results on 152 pairings).

The cause (**READ**): `accuracy = shot_accuracy_base + (shooter − 50) / 200`, an absolute scale. `docs/MATCH_SIMULATION.md` records the
football as calibrated when a generated-world probe measured 2.29 goals a match; the ratings have risen since (D6), the pivot did
not move, and goals rose with them. A controlled pair makes it a test: 80-rated everybody against 60-rated everybody scored 4.38
against 3.46 goals a match (ratio 1.26).

**Fix** (`shared::shot_accuracy`, `shared::defensive_quality`, both resolvers): the shooter is measured against the side defending
the shot, so a world rated twenty points higher scores no more; the base is recalibrated from 0.35 to 0.29.

| | Before | After |
| --- | --- | --- |
| Goals a match, followed leagues of three worlds | 3.9 | **2.7-3.4** (mean 3.18 over 18,432 study matches) |
| Draws | 15-19 % | **22.4 %** |
| 0-0 / five or more goals | 1 % / 52 % | **4.5 % / 21.9 %** |
| Shots on target a match | 10.8 | **6.3-8.0** |
| 80-rated v 60-rated worlds, goals ratio | 1.26 | **1.03** (test band 0.9-1.1) |

What it did not fix, and was not tried: the dormant tier (2.4-2.5) and the engine (about 2.9 in the followed leagues) still differ by
about half a goal; `engine::simulate` was changed to the same rule and has the same band test.

## 6. Formation suitability and tactical realism

### 6.1 Why the engine barely prices a formation (**READ**)

The five-zone engine reduces every player to one of four groups (goalkeeper, defender, midfielder, forward). A formation sets how
many are in each group. A contest in a zone draws **one** player at random from the group (`snap_player`) and compares his
attributes with one drawn from the other side's group. So a formation changes who can be drawn and the group's average, and nothing
else: there are no flanks, no width in the contest, no numbers in a zone, and no way for a winger, a full-back or a holding player to be
"used effectively" by a shape. A player out of position is already penalised, through the attributes he brings to the group he
is deployed in. What is missing is any value for fit to a slot beyond that. This is the engine's model, not a bug; M12 models
geometry.

### 6.2 The experiment (**RUN**, `p5_formation_matches`)

Each of 8 clubs per world (spread over the strength order) plays a fixed panel of 6 mid-table clubs, home and away, 8 repetitions
(96 matches per club and shape), in every one of the eight formations, then in its best-scoring shape with one tactical setting moved at
a time. Squads, opponents, venue and the opponents' own shapes never change. 3 worlds, 24 subject clubs, 18,432 matches, after a 60-day
warm-up so that AI tactics and conditions are those of a real match day. Standard errors are over **clubs**, never matches.

| Formation (club's own mean removed) | Points a game (± se over 24 clubs) | Goals for | Goals against |
| --- | --- | --- | --- |
| 3-5-2 | **+0.086** (±0.038) | +0.079 | −0.079 |
| 4-1-4-1 | +0.053 (±0.040) | +0.142 | +0.033 |
| 4-4-2 | +0.020 (±0.030) | +0.044 | −0.016 |
| 4-2-3-1 / 5-3-2 / 4-5-1 | 0.000 / 0.000 / −0.005 | | |
| 3-4-3 | −0.072 (±0.043) | −0.139 | −0.020 |
| 4-3-3 | **−0.082** (±0.037) | −0.179 | +0.016 |

- **No formation dominates.** The best is worth 0.09 points a game against about 1.4 an average; the worst −0.08. 3-5-2 against
  4-3-3 is 0.17 points (about 3 standard errors) and is the group-count effect of 6.1: shapes with more midfielders and fewer
  forwards do better here. A club's own eight shapes spread by 0.185 points a game (one standard deviation) against about 0.13 of
  pure match noise.
- **The squad model's formation score does not predict results:** within-club slope **−0.021 ± 0.021** points a game per rating point
  (P4.7's observational figure was −0.005), and the shape the model rates best is the best performer for 12 % of clubs (chance:
  12.5 %). Choosing a shape "for its squad-model score" is sporting only in the model's terms.
- **The shape AI clubs choose most is the one that scored worst here:** 4-3-3 (44 % of clubs at season 10 in the P4.7 formation study at this tree) scored −0.082 ± 0.037
  points a game (about 2.2 standard errors), because clubs choose by the squad model and nothing feeds results back into the choice.
- **Score distribution:** 3.18 goals a match, 4.5 % goalless, 21.9 % with five or more, 42.6 / 22.4 / 35.0 % win/draw/loss for the
  subject (the panel is mid-table, the subjects span the league); a reference season of the same worlds scored 2.48-2.56 goals a
  match and 26 % draws (including the dormant tier).

| One setting moved (against the club's neutral settings) | Points a game | Reading |
| --- | --- | --- |
| Play style **Possession** | **+0.125** (±0.029) | the largest single effect; a candidate exploit |
| Defensive line **very low** | **+0.070** (±0.024) | the known under-priced dials (the in-match AI rations them to two) |
| Play style Attacking / width Narrow | +0.054 / +0.053 (±0.03) | |
| Defensive line high | −0.039 (±0.019) | |
| Counter, HighPress, Defensive, pressing, tempo, wide | within ±0.04 | indistinguishable from none |

Not fixed here: a tactic worth +0.125 a game to every club that adopts it is the next exploit to look at; nothing was tuned to it.

### 6.3 What was not built, and why

A position-specific contest model (flanks, zone numbers, role geometry) would be a new match model with a new calibration, and is
M12's domain. The brief forbids inventing tactical realism through arbitrary bonuses; adding a fit bonus would have been exactly that.
The measurement above is the baseline such a model would have to beat.

## 7. Match-day AI

What the in-match AI already did (**READ**, `engine/src/ai.rs`): looks up at checkpoints (the interval, 60/70/80 by experience) and
at once for a sending-off; replaces an exhausted player at once; two goals down or one down from the hour chases (a forward for a
midfielder, the back line kept at three, attacking style, a high line, fast breaks); ahead from the eighty-first protects (a defender
for a forward, a defensive style, a low line); a Visionary reaches for a bolder shape; a keeper sent off is replaced by a keeper. It is
rationed so it cannot stack the under-priced dials.

| Item of the brief | Status |
| --- | --- |
| Starting XI selection | **Exists** (`ai_select_starting_xi_by`, manager rotation and youth trust) |
| Bench selection | **Exists** (`matchday_bench`, bounded, keeper kept) |
| Substitutions, fatigue management | **Exists**; unchanged |
| **Manager identity reaches the match** | **Added** (`ai_profile_for`): the top and bottom quarter of risk or adaptability decide the personality (bold: Visionary, cautious: Pragmatist, adaptable: Reactive), and `AiProfile.risk` moves *when* he chases one goal or protects a lead by up to ten minutes either way. Neutral (0.5) is the old rule. The user's own club and a club with no manager stay neutral |
| **Injury replacement** | **Added**: the engine recorded an injury and let the player play on; the manager now replaces him at the next look with the best bench player of his position (a keeper only by a keeper). **Limitation:** with no substitution left he plays on; the engine has no rule for a hurt player's reduced play and none was invented |
| Suspension never fielded | **Verified in both engines** (`neither_engine_fields_an_injured_or_a_suspended_player`, with a control that both start when fit) |
| Injured never fielded | **Verified**; the treatment-room top-up fills a short side only when nobody else exists, as before |
| Yellow-card management (taking off a booked player) | **Not implemented**; no consequence the engine could price |
| Match-state adaptation / defensive protection / controlled risk | **Exists**, now per manager |
| **Formation adjustment for unavailable players** | **Built (`matchday_formation`), measured, not enabled.** Twelve worlds, 96 club-scenarios per case, two best defenders / midfielders / forwards out: the rule's shape against the club's own is **−0.004 ± 0.015, −0.013 ± 0.015, +0.043 ± 0.018** points a game; one marginal result of three, and an engine that reads a formation as group counts cannot be shown to reward a fit. A decision whose consequence cannot be shown is not made on the club's behalf. To enable it: call it from `build_team_with_bench_from` for AI clubs **and** hand the shape to the M12 adapter, or delete the module |
| Different managers decide differently | **Verified at decision level** (`a_cautious_manager_protects_a_lead_earlier_than_a_bold_one` and three more, each red when the risk shift is zeroed); **no results claim**: appointment outcomes were not tied to results |

## 8. Youth, rotation, development (**RUN**, `p47_youth_causes`, 3 gate worlds × 10 seasons)

The chain, measured link by link: the cohort of 2,182 opening prospects falls to 1,247 at season 10 and flattens (1,293, 1,267, 1,247 over
seasons 8-10): the intake is 1-3 players a club a year (set by what an academy lacks, 424-704 new to the world a season) against 387-535
a season who mature out of the cohort, plus 32-172 who age out. **That is a steady state, not a defect, and it is unchanged** (P4.7: 1,195;
about 10 % is noise).

Of the prospects who are first-eleven ready by the group proxy, the senior-role ones play: 2,076-2,677 minutes on average, 21-29 % under 450. The
academy-role ones (6 → 28 a world) play **0 minutes**, by design: the match-day pool is the senior squad, and the promotion rule
(`youth_pathway`) promotes a player the squad model wants, which these are not. Prospects who are not ready (457-1,133 senior, 207-545
academy) play little, rightly: 85-92 % under 450. So the "share of prospects under 450 minutes" is a property of the population (most
of the cohort is not ready), not of the selection or the engine; forcing minutes to move it would be the quota the brief forbids.

Loans (`p47_loans`): 219 / 494 / 328 / 162 loans over seasons 1-3 / 4-6 / 7-9 / 10, **8 / 20 / 22 / 22 %** idle at 60+ days, 10.4 / 9.8 / 8.8 /
8.3 minutes per loan day (P4.7: 12 / 16 / 22 / 26 %, 10.1 / 9.8 / 8.6 / 8.0): **no change is claimed**; the residual cause is the same (a
loan that arrives behind the borrower's starters).

## 9. The P4.7 gameplay defects

| | Defect | Result |
| --- | --- | --- |
| **A** | A retiring starter left the user's saved eleven at ten, and **moved every later starter up a slot** (the eleven is slot-aligned, `retain` is not) | **Fixed on every departure path.** `refill_saved_eleven_slot` puts the best fit for the vacated slot among healthy match-day-eligible players in it, before the references are removed: retirement (deferred until the squad has aged), release/expiry, sale, loan out, loan return, loan-to-buy, a retired loanee. Same code for manual and delegated careers; a kick-off tops a still-short side up as before. **Red without the call** on each path (release, retirement, sale, loan return, loan-out, retired loanee) |
| **B** | Agreed deals waiting to register were not reserved: a second manual bid could spend the same budget, cash or wage room | **Fixed**: `pending_fee_commitments` and `BuyerWageFacts::of_committed`, read **at decision time only** (a registration is one of those deals, so reading it there would count it twice; `a_registered_deal_is_not_reserved_a_second_time` holds the line). No schema change: derived from the saved offers. Delegation's wage room already reserved; its cash room does not yet (listed) |
| **C** | Strategy reaching the listing decision | **Partly**: the reinvestment rule asks a contender for more than a small club on real squad-model numbers (red when the bar is lowered). The listing path that passes the profile to it is **still covered only by reading and an ordering test**: in the fixtures tried, a sale between the two bars is always also critical (sale of a first choice is never listed), and no honest witness exists. The test reviewer confirmed the claim by mutation |
| **D** | Formation persistence | **Verified**: `p5_formation_persistence` (SQLite round trip; the prepared match in both engines' inputs; the played match identical after reload); red when the match is prepared in another shape. It is a characterisation test: persistence already worked |
| **E** | Manager decisions | **Fixed and verified at decision level** (section 7); no results claim |

## 10. Cross-engine consistency (**RUN**)

The same prepared match is built once (`build_team_with_bench_from`) and handed to both engines. Tested: both name the same eleven in the same
slots, the same bench, the same formation for each side and the same number of substitutions (five), and neither fields an injured or
suspended player (`both_engines_are_handed_the_same_match`, red when the adapter ignores the club's formation). Scores need not agree and
do not. Not tested: the *play* of an M12 match against a five-zone one (rules such as extra time and shoot-outs are APFM's in both). M12
was not run in this phase's studies; nothing here validates M12's own football.

## 11. Long career

In `docs/APFM_P5_VALIDATION.md`. In one line: **invariants clean everywhere, no club in debt, reload twins identical, cost flat against P4.7**;
squad-quality measures unchanged (the trajectories differ because the engine does).

## 12. Reviewers

| Reviewer | Outcome |
| --- | --- |
| `ofm-architecture-reviewer` | 3 blocking, **fixed**: the loan-return path was unwired; `transfers_tests.rs` grew past its cap (tests moved, back to its base size); `project_transfer_bid_financial_impact_with_terms` and the legacy `resolve_shot` crossed or grew past 100 lines (helpers extracted). Non-blocking, fixed: `find_map` in injury replacement, `matchday_formation` says plainly it is not wired. Left: functions over 100 lines that were already over (`place_bid` 290 and three more, each grown by one to six lines, and the legacy `resolve_shot`, 109 before and 112 now); an outfielder can fill the goalkeeper slot of a saved eleven when no keeper is eligible (as at kick-off); `execute_transfer_with_terms` still removes by index from the buyer's eleven (rare); the crate-graph allowlist in `CLAUDE.md` lacks four workspace members (not this change's violation) |
| `ofm-dedup-reviewer` | 4 blocking, **fixed**: one `best_fit_for_slot` for three copies; `ManagerIdentity::is_clearly_bold/cautious/adaptable` as the one cutoff; one `condition_factor`; the loan-return call. Listed for epic #589: three scans of pending offers; the "fit and eligible today" fold in four places; six or seven copies of top-fourteen strength and four of the standard error in season-harness studies; delegation's cash room does not subtract pending fees |
| `ofm-test-reviewer` | Every fix-removal on the main scenarios killed (release, retire, sale, fee and wage reservations, injury replacement, risk shift, the 50-pivot, the adapter's formation, persistence). **Acted on:** loan-out and retired-loanee tests; exact elevens in two loan tests; a bench that discriminates fit from rating; the injured-reserve case; unit tests for `shot_accuracy` and `defensive_quality`; a one-shot-engine ratio test; the average-risk test split and widened to straddle its thresholds; play-style timing tests; `derive_personality` tie and boundary tests; positive control; refusal causes pinned. **Not acted on:** `p5_formation_persistence` is four scenarios in one test (labelled a characterisation test) |

## 13. Remaining work and the next step

1. **The one precise next step: give the engine a position-specific contest** (or measure M12) **before any more formation AI.** The
   baseline to beat is section 6.2: a squad-model score with slope ≈ 0 against results. Until then, formation choice is the squad
   model's opinion, and a match-day shape change is unmeasurable.
2. Possession style (+0.125) and a very low line (+0.07) are dominant dials in the engine; look at them before anything tunes a coefficient.
3. The dormant tier (2.4-2.5 goals) and the engine (about 2.9) differ by half a goal; a follower of a lower league sees a different
   football from the leagues around it.
4. The listing-side strategy test (section 9, C) needs a fixture the squad model can produce.
5. Delegation's cash room does not subtract pending fees (its wage room does).
6. **The GUI.** No person, and no agent, has played the window.
7. M12 was not validated; its own scoring and its geometry are outside this phase.

## 14. GUI and the push

**GUI: not performed.** This session has no desktop control, so nothing was launched, clicked or watched, and no part of the brief's list
(new career to continue career) is claimed verified from the window. `docs/APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md` was extended with section
11 (nine steps, each tied to behaviour this phase changed: the saved eleven after retirement, release, sale; a second bid against an agreed deal;
formation survival; Quick simulation against the M12 viewer; the goal rate; the hurt player's replacement). Every result cell is empty on purpose.

**Push: not done.** `gh repo view augustinpatte/ap-football-manager` reports the repository **PUBLIC**; the brief allows a push only to an existing
**private** repository, and this branch carries a football-data pipeline and generated-world work whose provenance notes
(`APFM_DATA_PROVENANCE.md`) are not for a public remote. Authentication works (`gh auth status`) and the branch does not exist on the remote, so a
push would be a new branch, but the visibility is the reason it was not made. The commits are local. If the repository is meant to be private,
making it so is the owner's decision; then `git push origin claude/apfm-playable-career` (no force) is the whole step.

## 15. Completion matrix

| Objective of the brief | Status |
| --- | --- |
| 1. AI clubs choose formations for meaningful reasons | **IMPLEMENTED BUT NOT VALIDATED**: the reasons are the squad model's; the model's score does not predict results (slope −0.021 ± 0.021) |
| 2. Tactical choices have measurable, plausible consequences | **INCOMPLETE**: measured (section 6.2); formation effects are small and come from group counts; two dials look dominant |
| 3. Positions and attributes affect performance | **COMPLETE AND VALIDATED for attributes** (relative quality decides a chance, tested); **INCOMPLETE for position-specific fit** (four groups, section 6.1) |
| 4. Credible match-day AI decisions | **IMPLEMENTED BUT NOT VALIDATED**: verified as decisions (timing, replacement, per manager); no consequence for results measured |
| 5. Injuries, suspensions, fatigue, substitutions | **COMPLETE AND VALIDATED** for eligibility in both engines, injury replacement and substitution limits (five in both); **a hurt player with no substitution left plays on** (engine limit) |
| 6. Youth, rotation, loans consistent with participation | **COMPLETE AND VALIDATED** as an analysis: no defect found, steady state documented, nothing forced; the loan residual is unchanged |
| 7. Long-term squad strategy sustainable | **COMPLETE AND VALIDATED**: no debt, invariants clean (validation doc) |
| 8. Deterministic, performant, save-compatible | **COMPLETE AND VALIDATED**: reload twins identical, cost flat, no schema change |
| P4.7 defect A (saved eleven) | **COMPLETE AND VALIDATED** |
| P4.7 defect B (commitments) | **COMPLETE AND VALIDATED** for manual bids; **IMPLEMENTED BUT NOT VALIDATED** for delegation's cash room (not changed) |
| P4.7 defect C (strategic sales) | **INCOMPLETE**: the listing wire is not behaviourally tested |
| P4.7 defect D (formation persistence) | **COMPLETE AND VALIDATED** |
| P4.7 defect E (manager decisions) | **IMPLEMENTED BUT NOT VALIDATED** at the level of results |
| Cross-engine consistency | **COMPLETE AND VALIDATED** for selection, formation, limits and eligibility; play not compared |
| Real application smoke test | **BLOCKED**: no desktop control; checklist ready |

**Decision: CONDITIONAL GO**, with the condition in section 13, item 1.
