# APFM I9: staff, delegation and training

Branch `claude/i9-staff-delegation`, based on `f31dfa26` (the I4 integration). Local only: nothing was
pushed or merged. The evidence is in [`APFM_I9_VALIDATION.md`](APFM_I9_VALIDATION.md).

I9 completes what was already a large system. The audit found the staff market, eleven duties, the
Prudent and Total presets, proposals, the history and the assistant's advice all in place and tested
(`APFM_STAFF_SYSTEM.md`, `APFM_DELEGATION_REPORT.md`, `APFM_ASSISTANT_COACH_DELEGATION.md`,
`GAME_SYSTEMS.md` § "Delegation: Prudent and Total"). Nothing there was rebuilt. The gaps that were
real, and what closed each:

| Gap found in the audit | What I9 did |
|---|---|
| Every duty belonged to "the assistant"; the manager could not put a particular coach, head of youth or head of scouting in charge, and a staff member's quality did not matter for any duty except through the assistant | **Duty owners** (`DutySetting::owner`, `staff_system/delegation_owner.rs`). |
| "Ask me" was shown for every duty although training, the lineup, tactics, match preparation and scouting never asked | `Duty::supports_approval`, shown by the backend; training now really asks (below); the other four are not offered a choice that does nothing. |
| Delegated training replayed the AI planner unchanged: blind to the coaches' strengths, to the week's fixtures (the stored plan said High while `training` ran Low), and to who was running it | `staff_system/delegation_training.rs`. |
| The Training tab showed only the stored plan | `training::training_outlook`, the `get_training_outlook` command and a card on the tab. |
| The assistant had no advice on the manager's own training plan, on the coaches' strengths, or on injuries | Three new recommendation kinds. |
| A weak youth or goalkeeping coach was worse than an empty seat (open point 1 of `APFM_STAFF_SYSTEM.md`) | Floored at neutral. |

## 1. Duty owners

A duty has one owner, the staff member whose rating the duty is judged with. The default (`None`) is
the assistant manager, which is what every saved game means: the field is `#[serde(default)]` inside
the `staff_delegation` JSON row, so there is no migration and no old save changes behaviour.

| Duty | Who may own it | What the owner's rating changes |
|---|---|---|
| Training | assistant, first-team coach, fitness coach | whether he reads the coaches' strengths (40), whether he may ask for High (35) |
| Match preparation | assistant, first-team coach | what he notices (min severity and number of items of the advice he acts on) |
| Scouting | assistant, head of scouting | what he notices of the squad's weak positions |
| Youth | assistant, head of youth, youth coach | which promotions he notices |
| Staff recruitment and renewals, lineup, player renewals and recruitment, tactics, offers | assistant only | unchanged: they need the whole squad, the books or the board |

* **One owner per duty by construction** (one setting per duty): two members can never both hold an
  exclusive responsibility. Naming a second replaces the first; naming the assistant stores nothing, so
  a later change of assistant cannot leave a duty pointing at someone who has gone.
* **Only fitting posts**: `set_duty_owner` refuses a post the duty does not list
  (`be.error.delegation.ownerNotEligible`) and a member who is not on the staff
  (`be.error.delegation.ownerNotFound`).
* **A departed owner** hands the duty back to the assistant, with one line in the history
  (`delegation.log.ownerLeft`), not one per run. A duty nobody in post can run cannot be handed over
  (`be.error.delegation.noAssistant`), and `run_after_day` leaves it alone rather than marking it done.
* The owner's rating enters through `assistant::recommendations_read_by(game, team, Some(rating))`:
  the same engine the assistant's advice uses, read with a different eye. There is no second advice path.
* A change of owner forgets what the previous owner last wrote, so the new one starts from the club's
  settings as they stand and does not read the previous plan as a manager override.
* Presets (`apply_mode`) and the manual pause keep owners: `set` mutates the existing setting.

## 2. Delegated training (`delegation_training.rs`)

The base plan is `ai_training::plan_for_team`, the plan every AI club runs, so the manager's club and
the others live by the same condition bands and there is **no second AI decision path**. The owner adds
what the club's staff and calendar really are:

1. **Fixtures.** Two or more matches in the coming week and the stored plan is Recovery at Low, the
   session `training` would run anyway (`CONGESTION_FIXTURE_THRESHOLD`, now `pub(crate)` and read from
   there). The manager sees the session he will get.
2. **Coaching strengths.** The Wednesday slot (`weekday % 5 == 2`) is the session the coaching staff
   teach best, measured by the very figure `training` applies to the gains,
   `compute_coaching_bonus(..).coaching_mult x specialization_mult`, if it beats the style's own session
   by 5 %. The other four sessions keep the club's style, so development stays balanced (Physical,
   Recovery and the style rotation are untouched). An owner below 40 does not look.
3. **Caution.** An owner below 35 never asks for High. A weak planner develops the squad less; he does
   not wear it out.
4. **Raising the load asks.** Easing is done at once (a recovery order is logged). Raising the intensity is
   the step that costs fitness, so with "ask me" on training it is a proposal
   (`delegation.proposal.trainingLoad`, a `SetTraining` action); until the manager answers the session
   runs at the standing intensity. At most one is waiting. **One answer covers a month**: a refusal
   stands the duty back for 28 days, and an approval lets the owner raise the load again, unasked, for
   28 days. (The first version asked a week after each answer and raised about 23 questions a season in
   the ten-season study, because the condition band the plan reads crosses the High threshold many times
   a year; with the window it is about ten.) An approval is written as what the staff last wrote, so
   the next day does not read it as a manager override. Acting alone (no "ask me") raises it at once.

What is preserved: the fatigue guard (40), the near-match taper, the congested-week rule, and the
manager-override stand-back (`delegation_control`). The planner never writes the club's stored
schedule or groups.

## 3. Training outlook

`training::training_outlook(game, team)` (and the `get_training_outlook` command and
`TrainingOutlookCard`) report, from the same functions `training` runs: whether today is a session day,
the intensity today's session really has after the taper, the fixtures in the next seven days, the
coaching and recovery effects of the staff, the likely starters' condition and who sets the plan (and
whether a proposal is waiting). The screen restates no rule: the taper comes from `tapering_teams`, the
coaching figures from `compute_coaching_bonus`.

## 4. Assistant advice

Three new kinds in `assistant.rs`, each computed from the club's data and carrying the figures:

| Kind | When | Action |
|---|---|---|
| `TrainingLoad` | the manager runs High, the likely starters average under 65 and the week is not already a heavy one (which has its own advice) | Medium on the same focus; Recovery/Low under 45 (severity 3) |
| `CoachingStrength` | the coaches teach another session at least 7 % better than the focus being run, the load is not heavy, the assistant reads the coaches (coaching rating 40) | that focus at the current intensity (severity 1) |
| `InjuryRecovery` | two or more injured, or one out for 21 days or more | informational; severity 2 if there is no medical department worth the name |

Advice is still deterministic, still filtered by the reader's eye and capped by it (`max_items`), so a
poor assistant raises less and a duty owned by a better reader notices more. Accepting a
recommendation is the existing `apply_assistant_recommendation`; refusing it changes nothing.

### 4b. The squad floor under Total authority

The game never fills the human's club to the floor of 15 seniors (it warns him once a season): a human
manager is expected to sign players. A manager who has handed the squad to the assistant has no one
doing it, and the ten-season study found a Total-delegated club that sat 70 million in credit with the
squad at 14 and, in a first version of the study where proposals went unanswered, at 10. Under **Total
authority with "ask me" off on player recruitment** the assistant now runs
`squad_floor::restore_minimum_squad` every day the duty is delegated (`delegation_squad::keep_the_floor`):
the academy first, then the free agents the club can pay, under the board's wage policy and the
senior-place limit, which yield only where the club would otherwise be below the floor; nobody is
created, each player brought in is in the history (`delegation.log.floorPromoted` / `floorSigned`), and a
second run does nothing. Under Prudent and when the duty is not handed over it is the manager's, as
before. It is the AI clubs' own routine called for the user's club, not a second decision path.

## 5. Staff effects

Unchanged in size (see `APFM_TRAINING_VALIDATION.md`): coaching 0.85 to 1.35 and +25 % for a specialist,
recovery up to x1.4 from the medical department and +8 % from the fitness coaches, youth +/-8 %,
goalkeepers +/-12 %, injury prevention up to 12 %, injury length x1.25 to x0.80. One change: the youth and
goalkeeping multipliers are floored at 1.0, so below-average coaches are no help rather than a harm,
and the order none <= weak <= average <= excellent now holds for every effect
(`staff_effects_rise_from_weak_to_average_to_excellent`, which used to exempt these two).

## 6. User experience

* **Staff > Management**: each duty row shows who is in charge. Where more than one member could be,
  a selector (the member, post and rating) puts him in charge or hands the duty back to the assistant;
  where only the assistant can, his name is shown; where nobody can, it says so and the duty cannot be
  handed over. "Ask me" is shown only where the duty has something to ask. Proposals show the focus and
  both intensities by name.
* **Training tab**: the outlook card.
* Everything is wired: `set_duty_owner`, `get_training_outlook`, `get_staff_overview.duties`. New text is
  in all 12 locales (30 keys); the i18n gates pass.

## 7. Existing systems reused

Duty settings, proposals, history and stand-back (`delegation_control`), `authorize` and the finance
bound, `market` (hire, renew, release, severance, budget), `assistant::recommendations`,
`ai_training::plan_for_team`, `training` (taper, fatigue guard, coaching and recovery maths),
`squad_floor` and the I4A senior hold (every route is untouched), the fast-forward stop on a waiting
proposal (a training proposal is an ordinary proposal and stops it like any other).

## 8. Shared files, for the integration engineer

Nothing in the tactical engine, `ai_tactics`, formation evaluation, `turn/` or M12 was changed. Files
shared with other work that I did touch, all additively:

| File | Change |
|---|---|
| `domain/src/delegation.rs` | `DutySetting::owner` (`serde(default)`), `Duty::supports_approval`, `set_owner` |
| `ofm_core/src/training.rs` | `compute_coaching_bonus` and `CONGESTION_FIXTURE_THRESHOLD` are `pub(crate)`; youth and goalkeeping multipliers floored at 1.0; `mod outlook` |
| `ofm_core/src/ai_training.rs` | `likely_starters_condition` is `pub(crate)` (visibility only) |
| `ofm_core/src/staff_system/delegation_squad.rs` | `keep_the_floor` (calls `squad_floor::restore_minimum_squad`) |
| `ofm_core/src/staff_system/{assistant,delegation}.rs` | `recommendations_read_by`, the duty loop resolves owners, three advice kinds; `NextMatch` and `next_match_and_load` are `pub(crate)` |
| `src/commands/{staff,squad}.rs`, `src/lib.rs` | `set_duty_owner`, `get_training_outlook`, `StaffOverview::duties` |
| `src/i18n/locales/*.json`, `INTENTIONAL_SAME.json` | 30 keys |

If I5 changes `ai_training::plan_for_team` or the tactical duty, the contracts to keep are: the planner
returns `Option<(TrainingFocus, TrainingIntensity)>` and is the single planner for every club; the
tactics duty is `delegation_control::tactics` calling `ai_tactics::plan_for_club`.

**An I5 observation that is not a finding.** `ai_tactics` decides whether a side can press from the
squad's *stamina attribute* (`SquadReading::legs`), not its current condition. A two-season probe suggested
that the delegated tactics duty cost about seven points of starters' condition on the eve of a match
(80.1 against 88.0 without it). **That did not reproduce** in the controlled ten-season study
(`APFM_I9_VALIDATION.md` §3: 86.2 with the tactics duty delegated, 86.8 without, inside the noise of one
seed), and the probe's two arms differed in more than the duty. I record the code observation, not a
fitness cost: if I5 reworks the planner, reading condition as well as stamina would be a natural
refinement, and the study can be re-run on the tactics arm to check it.

## 9. Remaining limitations

* Duty owners change the *reading* of a duty (what it notices, whether it dares a High session), not
  its authority: money and squad rules are still `authorize`, the board and the squad floor. That is
  deliberate: delegation never bypasses a validation.
* Match preparation, scouting and youth read through the advice engine, so an owner's effect there is
  the sharpness of what he notices, not a different set of actions.
* The full FM24 staff attribute sheet is still not displayed on the staff cards (`APFM_STAFF_SYSTEM.md`
  open point 3); the screen shows post, rating, contract, wage and the new duty row.
* No MCP tool exposes `set_duty_owner` or the outlook (the `_internal` functions `set_owner` and
  `get_training_outlook_internal` are there if one is wanted).
* AI clubs use the unchanged planner. The staff effects apply to them through `training` as before, and
  their staffing is the existing `staff_system::ai`; I9 did not change AI staffing.
* The Tauri app was not launched (no GUI run in this environment).
