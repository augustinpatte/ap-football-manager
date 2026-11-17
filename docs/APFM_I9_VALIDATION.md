# APFM I9 validation

Branch `claude/i9-staff-delegation`, base `f31dfa26`. Design and behaviour:
[`APFM_I9_STAFF_DELEGATION_TRAINING.md`](APFM_I9_STAFF_DELEGATION_TRAINING.md). Nothing was pushed or
merged; `apfm/main` and the I5 worktree were not touched; no tactical file (`ai_tactics`, `turn/`, the
engine, formation evaluation, M12) is in the diff.

## 1. Gates

Run from the repository root on the final tree (`2c0dfb7f`).

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | exit 0 |
| Clippy | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings` | exit 0 |
| Clippy, MCP | the same with `--features mcp` | exit 0 |
| Rust workspace | `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace` | exit 0; 155 result lines; **5,139 passed, 0 failed, 125 ignored** (baseline 5,103 / 124) |
| Frontend suite | `npm test` | exit 0; 348 files passed, 1 skipped; **3,066 passed, 4 skipped** (baseline 345 files, 3,053) |
| Frontend build | `npm run build` (`tsc && vite build`) | exit 0 |
| Lint, format | `npm run lint`, `npm run format:check` (Biome 2.5.5, confirmed the real package) | exit 0 (lint reports 3 infos) |
| Dead code | `npm run knip` | exit 0 (two configuration hints, not findings) |
| i18n | `npm exec --no -- vitest run src/i18n` | pass (30 keys in 12 locales) |

New tests: **36 Rust** (5,139 − 5,103) and **13 frontend**. The one new ignored test is the study in §3.

| Area | New tests |
|---|---|
| Duty owners (`delegation_owner`) | 6: default owner, named owner's rating, ineligible and unknown refused, one owner per duty, owner leaves (once), candidates ordered |
| Delegated training (`delegation_training`) | 13: ask-me raises are proposals and not repeated; acting alone raises at once; easing never waits; an approval is not read as a manager override; a refusal holds a month; an approval covers the month, an old one does not; a congested week is recovery; the midweek slot follows the coaches; a weak owner neither tailors nor asks for High while a strong named owner tailors; determinism; the manager's own setting stands |
| Assistant advice (`assistant`) | 4: a plan too hard for the squad (and the crisis variant), no double advice in a congested week, the coaches' strongest session (and silence for a weak reader), injuries |
| Outlook (`training::outlook`) | 4 (+ the command test in `commands/squad.rs`) |
| Squad floor under Total | 4: restored once, Prudent and manual left alone, nobody created from an empty market, kept on a day the recruiter is not due |
| Commands and persistence | owner command and overview projection (`commands/staff.rs`); owner survives SQL round trip, and a save from before owners loads with the assistant in charge (`db`) |
| Staff effects | `staff_effects_rise_from_weak_to_average_to_excellent` now asserts the order from *none* for youth and goalkeepers (the old assertion exempted them); it fails on the old code (a weak youth coach was x0.968 against x1.0 for none) |
| Frontend | owner selector both ways, "ask me" shown only where meaningful, "nobody can run this", training-load proposal params, the outlook card (4), its helper, the hook (2), the two service calls |
| Save/reload, determinism (`season-harness`) | `a_reloaded_delegated_career_does_not_repeat_what_it_did` |

The reload test saves a delegated career (a named coach in charge of training, Prudent preset, proposals
answered) to SQLite at day 60, reloads it, plays 30 more days from the reloaded game and from the game that
was never saved, and requires **identical delegation state**; it first asserts that the delegation had
really done something (a proposal or history, and a written training plan), so equality is not vacuous. It
then plays on through the rollover and requires the owner and mode to survive, and no two proposals to
share an id.

Regression effectiveness was checked by construction for the behaviours that replace existing code (the
youth/goalkeeper floor fails the strengthened assertion on the old multiplier). I did **not** run the
remove-the-fix-in-a-disposable-copy procedure for every new test.

## 2. What the study is

`crates/season-harness/tests/i9_staff_training.rs::i9_staff_training_study`, ignored, measurement first.
Same generated `gate` world (80 clubs, seed 1), 10 seasons per arm, run with

```text
I9_SEASONS=10 I9_SEEDS=1 cargo test --locked -p season-harness --test i9_staff_training \
  -- --ignored --nocapture i9_staff_training_study      # 888 s for seven arms
```

In every arm the club's administration is delegated (renewals, recruitment, academy, offers, lineup, match
preparation) so the squad does not decay (see §4), and the staff duties are *off* so the staff stay as set.
The arms vary the staff (assistant held at 50 throughout) and who plans the training and the tactics. A
policy stands in for the manager: it accepts every administrative proposal and answers training-load
proposals every other time by number, so approval and refusal are both exercised. The assertions are
safety only: no day above 30 seniors or below the floor of 15, no invariant violation, and a delegated
training arm reaches matches within five points of the manager's own condition with at most one match in
ten below 60. "On the eve" is the condition of the eleven best healthy seniors the day before each of the
club's matches: what is actually fielded. The weekly sample is the figure the AI clubs are compared on.

## 3. Results (seed 1, 10 seasons)

| Arm | Starters, weekly mean | On the eve of a match: mean (below 70 / below 60) | Injuries / season (days lost) | Points, mean (finishes) |
|---|---|---|---|---|
| manual training, average staff (50) | 83.3 | 86.7 (7 % / 2 %) | 18.7 (278) | 79.4 (1st to 3rd) |
| manual, no coaching or medical staff | 77.1 | 81.2 (17 % / 5 %) | 17.2 (230) | 64.2 (1st to 9th) |
| manual, weak staff (30) | 82.8 | 87.2 (3 % / 1 %) | 16.2 (275) | 79.1 (1st to 4th) |
| manual, strong staff (90) | 89.3 | 93.2 (1 % / 0 %) | 15.9 (170) | 82.6 (1st to 5th) |
| training delegated, acts alone | 82.0 | 86.8 (4 % / 1 %) | 16.3 (212) | 76.0 (1st to 5th) |
| training delegated, ask me | 85.5 | 89.0 (4 % / 0 %) | 18.1 (218) | 80.7 (1st to 6th) |
| ask me, re-run after the one-month window (§4.4) | 82.0 | 86.1 (4 % / 1 %) | n/a | n/a |
| training and tactics delegated | 81.7 | 86.2 (5 % / 1 %) | 17.6 (215) | 79.3 (1st to 5th) |

The AI clubs' weekly starters' condition is 76.1 to 76.3 in every arm.

**Staff quality matters, and is not overwhelming.** Removing the coaching, fitness and medical departments
costs about five points of match-eve condition and lets 17 % of matches be played below 70 (7 % with an
average staff); a strong staff (90) adds about six and nearly removes it (1 %), cuts injury days lost
from 278 to 170 (the other arms are not ordered on injury days: 230 with no staff, 275 weak), and
wins a few points. Weak (30) and average (50) are not separable from each other in
one seed: 87.2 against 86.7, inside the noise (§6). So the benefit is real at the extremes and balanced:
a club with no department, or an excellent one, differs by a dozen condition points, not by a different
sport. **Staff wages were zero in this world (the staff bill reads 0 against a 49,786 budget), so the
cost side of hiring is not measured here**; it is covered by the `market` tests (budget, signing fee,
severance) and the Management panel.

**Delegated training keeps the squad as fit as the manager does.** Acting alone it reaches matches at
86.8 against 86.7 by hand, with fewer matches below 70 (4 % against 7 %) and fewer injury days lost (212
against 278). Under "ask me" it reached 89.0 in the first run and 86.1 after the month window: both are
level with or better than by hand, with no match below 60 in the first and 1 % in the second. At no point
in 10 seasons (about 450 matches per arm) is a delegated plan the cause of a fitness collapse; the worst
eve-of-match sample of a delegated arm is 46 to 61.

**Attribute progression by age** (rating change per season, the club's players against the AI clubs'; the
club has 14 to 18 players in the 19 to 21 band and 31 to 40 in the 22 to 25 band, per arm):

| Age | AI clubs | club, manual avg | none | weak | strong | training alone | ask me | + tactics |
|---|---|---|---|---|---|---|---|---|
| 19 to 21 | +1.97 | +1.31 | +0.80 | +0.71 | +0.44 | +1.50 | +1.00 | +1.00 |
| 22 to 25 | +0.52 | +0.54 | +0.63 | +0.63 | +0.46 | +0.45 | +0.68 | +0.47 |
| 26 to 29 | -0.02 | +0.12 | +0.10 | +0.03 | 0.00 | +0.12 | +0.06 | +0.12 |
| 30 to 32 | -0.82 | -0.72 | -0.65 | -0.70 | -0.76 | -0.74 | -0.73 | -0.67 |
| 33 to 35 | -1.6 | -1.57 | -1.40 | -1.50 | -1.54 | -1.62 | -1.25 | -1.29 |

The shape is plausible in every arm (growth to 25, flat in the late twenties, decline from 30, steeper
at 33) and the club's curve tracks the AI clubs' from 22 upwards: no arm shows runaway growth or collapse
over ten seasons. The effect of staff quality or of delegated training on development is **not resolvable
at these sample sizes**: the differences in the 19 to 21 row (+0.44 to +1.50) are the size of the noise of
14 to 18 players, and the "strong staff" arm has the lowest figure. I make no claim that staff quality
improves progression; the claim the data supports is that it does not distort it. This is rating (ovr),
not a per-attribute measurement, so unbalanced growth of one attribute against another is not measured.

**Delegation decisions.** The decision history keeps the last 120 entries, so these are the entries still
in it at the end and the counters the game keeps, not season totals. Acting alone: 41 training entries;
ask me, first run: 15 approved, 13 declined, 32 training entries; ask me, re-run after the window: 10
approved, 3 declined, 34 training entries; tactics too: 27 training entries. The arms with no training
question raised 153 to 177 proposals in all, which is the administration's own load. The first "ask
me" run raised 404 proposals (about 23 training questions a season); after the one-month window (§4.4)
the same arm raised 268, about 10 a season.

**Rules held, every arm, every season.** Senior-place hold: 0 days above 30 seniors. Squad floor: 0 days
below 15 seniors. Overdraft: the lowest cash of the club was its starting cash (7,504,890) in every arm,
i.e. it never fell below where it began. `Invariants` (the harness's world rules) clean in every arm.
Season rollovers: the study does not assert that the delegation settings survive each of the ten
rollovers; the reload test (§1) asserts the mode and the named owner after one, and the existing
`a_fully_delegated_season_survives_reload_and_rollover` covers the settings, limits and history.

## 4. What the study found along the way

These are the things that were wrong, or that I got wrong, on the way to the table above.

1. **A neglected club decays; the first design measured that.** With nothing delegated, the user's club
   finished 20th from season 5 and sat below the squad floor for 1,277 days: nobody renews or recruits in a
   harness whose manager makes no decisions. That is not a game defect, but it would have drowned the
   staff and training signal, so every arm now has the same delegated administration (§2).
2. **A Total-delegated club could still go below the floor.** With the administration delegated and every
   proposal answered the squad sat at 14 to 17 seniors, touching 14; in a probe that left proposals
   unanswered it fell to 10 with 70 million in credit. The game deliberately does not fill the human's club
   (it warns him), so under Total authority the assistant now does what the game does for AI clubs
   (`keep_the_floor`, daily; design doc §4b). After it, 0 floor-breach days in all seven arms.
3. **The arms must hold the staff fixed.** In one run the "no staff" arm looked as fit as the average one,
   because the delegated staff-recruitment duty re-hired the coaches. The staff duties are now off in every
   arm. (A reading of that arm that said "staff do not matter" would have been wrong.)
4. **"Ask me" asked too often.** 404 proposals (about 23 training questions a season), because the
   condition band the plan reads crosses the High threshold many times a season. Now one answer covers a
   month (refusal holds, approval allows further raises): 268, about 10 a season, fitness unchanged.
5. **A claim I made and retracted.** A two-season probe showed the delegated tactics duty costing about
   seven points of match-eve condition (80.1 with it, 88.0 without). In the controlled ten-season study it
   is 86.2 against 86.8, inside the noise, and the probe's arms differed in other ways. I wrote the claim
   into the design document first and corrected it there; it is **not** a finding. The code observation
   that the tactical planner reads the stamina attribute, not the current condition, stands.

## 5. Determinism, save and reload, existing saves

* Delegation and training have no new random source: the planner is `plan_for_team` plus deterministic
  rules (`the_plan_is_deterministic`, `the_lineup_is_deterministic…`), and the owner's blur uses the
  existing seeded hash.
* The reload test above proves a reloaded career follows the career that was never saved, to identical
  delegation state, with a named owner, Prudent proposals and the training plan.
* `DutySetting::owner` is `#[serde(default)]` inside the existing `staff_delegation` JSON row, so there
  is no migration; a save from before owners loads with the assistant in charge of every duty
  (`a_delegation_saved_before_owners_loads_with_the_assistant_in_charge`) and the previous tests for old
  saves still pass. The two new commands add no stored field.

## 6. Limits of this evidence

* **One seed.** Seed 1 of the `gate` world, 10 seasons. The study accepts `I9_SEEDS=1,2,3`; I did not run
  more. One arm moved from 89.0 to 86.1 match-eve condition between two configurations that differ only
  in the proposal window, so differences of two to three points between arms are within the noise and are
  not read here. The ordering none < average < strong is outside it; weak against average is not.
* **Points** are a result of one club in one league over ten seasons and vary by 10 to 20 between similar
  arms; I report them and draw nothing from them beyond "no arm is damaged".
* **Staff wages were 0** in the study world (§3), so staff cost under delegation was not measured in a
  career; the delegated hire/renew paths and their budget ceilings are the existing, tested `market` and
  `authorize` code.
* **Rating only** (not attributes) for progression; the user's club is one club of 80.
* **Not run:** the Tauri app (no GUI run in this environment); the imported FM24 world; the `shipped`
  and `lower` worlds; the `/preflight` procedure and the five reviewer agents named in `CLAUDE.md`
  (`ofm-architecture-reviewer`, `i18n-auditor`, `ui-accessibility-reviewer`, `ofm-dedup-reviewer`,
  `ofm-test-reviewer`): the mechanical gates above ran, the review passes did not. The 11 non-English
  locale strings for the new keys were written by me, not by a native reviewer.
* Release-profile timing was not measured: the study ran in the repository's dev profile (which is
  optimised); 127 s per arm of ten seasons.

## 7. Readiness for integration

I see no release blocker in the delivered surface. The review passes in §6 should run before a PR, and a
multi-seed run of the study (`I9_SEEDS=1,2,3`, about 45 minutes) is the most useful further evidence. For
I5: the shared files are listed in the design document §8; the planner contract to keep is
`plan_for_team -> Option<(TrainingFocus, TrainingIntensity)>` for every club, and `delegation_control::tactics`
calling `ai_tactics::plan_for_club`; the study's tactics arm is the check to re-run if the tactical planner
changes.
