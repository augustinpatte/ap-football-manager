# APFM playable-career implementation report

Branch `claude/apfm-playable-career`, starting at HEAD `5f5a4dfc`. Inputs: `APFM_CLAUDE_TAKEOVER_AUDIT.md`,
`APFM_PLAYABLE_CAREER_GAP_MATRIX.md`, `APFM_PLAYABLE_CAREER_IMPLEMENTATION_PLAN.md`.

## Verdict

**The mission is not complete, and this report does not claim it is.** Two slices landed and a
backend journey test was added. APFM has **not** been played through its window by anyone: no
graphical session was launched, so "a new player can complete two seasons from the GUI" remains
unverified. `APFM_MANUAL_GUI_ACCEPTANCE_CHECKLIST.md` is written and every cell in it is blank.

## Evidence labels

* **UI-test** — Vitest/Testing Library, run by me.
* **BE-test** — Rust test through the application layer (the same `_internal` functions the Tauri
  commands call), run by me. No window, no IPC serialisation, no component.
* **Manual — NOT DONE** — needs a person.

## Slices

| # | Slice | Commit | Tests | State |
|---|---|---|---|---|
| 0 | Tactics: advanced phase dials folded by default (gap G1) | `5f5a4dfc` (already at HEAD before this session; the audit and plan were written against `92b2c616` and are stale on this point) | UI-test `TacticsTab.test.tsx:304`, in the green full run below | Done. Not re-implemented. |
| 1 | Dashboard alert for pending delegation proposals (gap G7) | `9ef59840` | UI-test: two new cases in `dashboardHelpers.test.ts` (alert with count and Staff target; none when empty or absent), seen failing first for the predicted reason | Done in code |
| 2 | Journey test through the application layer (plan stage 3) | `c2533ba6` | BE-test, 9 tests (8 in the default suite, 1 `#[ignore]`d) in `src-tauri/src/application/playable_journey.rs` | Done, with the limits below |

### Slice 1 detail

No backend change and no save-schema change. The audit assumed the proposal count did not reach the
dashboard; reading the code showed the full `Game`, including `delegation.proposals`, is already
serialized to the frontend, and `resolve_delegation_proposal` returns the game, which the Staff tab
hands to the dashboard's `setGameState`. Only the TypeScript type omitted the field. The change
widens `GameStateData.delegation`, adds one alert (`delegation_proposals`, warn, target tab `Staff`)
and the string `dashboard.alerts.delegationProposals` in all 12 locales. The alert clearing after a
resolve follows from that data flow (READ); it is not covered by a component test that clicks
through it.

### Slice 2 detail (what the journey tests assert)

| Test | Journey step |
|---|---|
| `squad_and_tactic_choices_are_kept_and_survive_a_reload` | preset, formation, a bench swap holding its slot, save, reopen |
| `an_unknown_formation_is_refused_without_changing_the_club` | invalid input, translation-key error |
| `a_live_match_cannot_be_advanced_past_or_finalised_twice` | duplicate-simulation guard; M12 interactive match finalised once |
| `a_delegated_match_day_finalises_the_match_once` | delegate path; no live session left; at most one result per day |
| `a_transfer_is_scheduled_on_acceptance_and_completes_on_its_registration_date` | bid, player terms, dated signing, squad and cash change once |
| `a_bid_above_the_transfer_budget_is_refused_and_changes_nothing` | finances respected |
| `a_pending_proposal_is_counted_until_the_manager_answers_it` | the number the new alert reads falls to zero on resolve |
| `delegation_needs_an_assistant_but_taking_control_back_does_not` | delegation authorisation |
| `a_reloaded_career_completes_two_seasons` (`#[ignore]`, release) | 30 days, reload, season 1, save/reopen, rollover, season 2, save/reopen, rollover, save/reopen |

The two-season test was shown to fail when the rollover call is removed ("the new season starts with
fixtures to play"), then restored.

Limits of slice 2, stated plainly:

* It plays the two seasons in `delegate` mode, i.e. the in-house `engine`, not M12. The M12
  interactive path is exercised once, in the live-match test.
* It does not assert that tactic changes reach the match simulation. That rests on the F4 report
  (earlier, not re-run).
* The "approval / automatic / manual" delegation behaviour is covered by the existing
  `delegation_scenarios`, `delegated_career` and `delegated_recruitment` suites (audit, DOC); the new
  tests only add the no-assistant refusal and the proposal resolve.
* A transfer is bid for and signed in one test; negotiation counter-offers, loans, renewals and free
  agents are covered only by the existing `transfers_tests` and the audit's earlier runs.
* The test world is the 80-club gate world, not the shipped 440-club world.

## Verification run (this session, at the commits above)

| Check | Result |
|---|---|
| `npm run build` | pass |
| `npm test` (full) | 332 files passed, 1 skipped; **2,923 passed**, 4 skipped (baseline 2,920) |
| `npm run format:check`, `npm run lint` | clean |
| `cargo fmt --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean |
| `cargo test --locked --lib` (root package) | **562 passed**, 0 failed, 8 ignored |
| `playable_journey::a_reloaded_career_completes_two_seasons`, `--release -- --ignored` | passed, 12.7 s run time after a 3 min 26 s cold build |
| Same test with the rollover removed | failed, for the intended reason |

**Not run:** the full Rust workspace suite (122 targets; last run at `92b2c616` per the audit, 4,698
passed), `cargo test/clippy --features mcp`, `knip`, the `preflight` script, `cargo deny`,
`cargo machete`, the reviewer agents (`ofm-test-reviewer`, `i18n-auditor`, `ui-accessibility-reviewer`,
`ofm-dedup-reviewer`, `ofm-architecture-reviewer`) that `CLAUDE.md` makes mandatory before a PR, and
`/preflight`. Slice 1 touches an i18n surface, so `i18n-auditor` is owed. Do not open a PR on this
evidence alone. No GUI of any kind was run.

## Findings made while writing the tests

1. **A reload can reorder the starting eleven.** `canonicalize_game_starting_xi_ids`
   (`crates/db/src/save_manager.rs`, commit `e152eba6`) swaps a mirrored side pair (LB/RB, LWB/RWB,
   LM/RM, LW/RW) whenever the swap raises positional fit, on every write and every load. The save
   writes a canonicalized *copy*, so the live game keeps the manager's order and the reloaded game
   shows the swapped one. I hit it by accident (arbitrary defenders in the back four; slots 1 and 4
   swapped). It is deliberate and has its own tests, so I changed nothing, but a manager who
   deliberately plays an inverted full-back will see it undone after reload. **Owner decision needed**
   (see below). The journey test now builds a position-sensible eleven so it does not trip this, and
   the checklist (step 2.6) asks a human to record it.
2. **The first fixture of a new career is a July preseason friendly**, which does not count for the
   league table (`Fixture::counts_for_league_standings` is `League` only). Any UI or test that reads
   "table played" after the first match will see 0. Not a defect; my first assertion was wrong.
3. **A signing completes on its registration date, not on acceptance** (`response.registration_date`,
   scheduled one day out in the test). The screens must say so; checklist 6.4 checks it.
4. **The `delegate` advance response carries no `fixture_id`**; only the `live_match` action does.
   Anything on the UI that infers "a match was played" from `fixture_id` would miss a delegated match.
   Not investigated in the frontend.
5. **The transfer-budget guard works** (`be.error.transfers.transferBudgetTooLow`); a bid above budget
   changes nothing.
6. **The takeover plan was stale** on G1 (already done at HEAD). The plan file now says so.

## Coverage of the requested journey

| Flow | Automated evidence | Missing |
|---|---|---|
| Create game, pick club, dashboard | existing UI-tests and BE-tests (audit); journey tests begin from a generated career | GUI |
| Squad, formation, XI, save | BE-test (new); `SquadTab` UI-tests (audit) | GUI; finding 1 |
| Tactics basic/advanced, presets, edit | UI-test (G1); preset applied in BE-test | proof that tactics change match results in M12 re-run; GUI |
| Calendar, no duplicate simulation | BE-test (new) | GUI double-click behaviour |
| Match prep, finalisation, result screen | BE-test: interactive finish once; existing `PostMatchScreen` UI-tests | GUI, post-match screen against real data |
| Transfers, contracts | BE-test: bid to dated signing, budget refusal; existing suites | counter-offer, loan, renewal, free agent through this layer; GUI |
| Delegation, pending decisions | alert (UI-test), resolve (BE-test), existing delegation suites | GUI click-through of alert and accept/reject |
| Inbox deep-links | existing UI-tests | whether each action lands on the right tab (unverified) |
| Save and reload | BE-test around SQLite, 4 save/close/reopen points in two seasons; existing four-season reload suite | GUI save/load buttons |
| Season rollover x2 | BE-test (new, ignored) and `season_lifecycle` (existing) | GUI end-of-season screens; M12 matches across seasons |

## Not started, and why

* **Column chooser (G2).** Plan stage 5 says it touches the settings schema and needs an owner
  decision first. Not begun.
* **Model versus picker (F1), engine versus M12 study (F4), shipped-world performance (P1/F3).** Owner
  decision or multi-hour studies on a quiet machine. Not run.
* **Kit pattern and jersey number UI (G6).** Cosmetic. Not started.
* **Inbox deep-link audit** and **hooks of alerts to a component-level click test.** Not done.

## Decisions needed from the owner

1. Finding 1: should the load-time mirrored-pair swap keep running for every load, run only for
   saves written before the canonicalisation existed, or be removed?
2. Plan stage 5 (column chooser): may the settings file gain a field?
3. Audit §9.1 and §9.3 (model/picker, `delegate` using `engine`) are still open.

## Next exact slice

Stage 4 of the plan: a person runs the checklist with `scripts/gui-validation/run-isolated.sh` and
files defects. Nothing further should be described as "verified" before that. If a GUI session is
not available, the next automatable slice is a component test that mounts `Dashboard` with a game
whose `delegation.proposals` is non-empty, clicks the alert, and expects the Staff tab, then resolves
the proposal and expects the alert gone; it needs the existing Dashboard test harness, which I did not
inspect.
