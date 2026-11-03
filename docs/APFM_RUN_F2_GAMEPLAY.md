# Run F2 — ratings, negotiations, loans, M12 match views, delegation audit

Branch `claude/run-f2-gameplay`, from the two F2 commits that were already pushed
(`39701531` matchday bench cap, `b2cb184b` FM24 rating from Current Ability, `WORLD_REVISION` 6).
Nothing was merged into `integration/gameplay-runs-a-d` or `apfm/main`; no production save, world
file or database was modified (every import, replay export and measurement wrote to a scratch
directory; the user's installed world was only **read**).

This report separates what was **done and verified**, what was **done but only partly verified**,
and what is **not done**. Where a claim rests on a particular test, the test is named.

---

## 0. State found at the start

| Question | Finding |
|---|---|
| Branch / tree | `claude/run-f2-gameplay`, clean, equal to `origin/claude/run-f2-gameplay` (`b2cb184b`). |
| The Cloud session's uncommitted transfer-timing and loan work | **Not recovered.** No stash, no extra worktree, nothing on the remote. Phases 2 and 3 were built from the committed state, reusing what was valid there (`PendingRegistration`, `registration_date`, the daily registration processor, the F1 player-consent step). |
| Was the real M12 engine available? | Yes. `~/apfm-match-lab` is a path dependency of `ofm_core`, with no stub marker. The F2 commit messages said M12 "was not run"; it has now been, see §6. |
| Real FM24 source data | `~/apfootballmanager/data/world.db` (read-only, SHA-256 unchanged before and after every run). |

---

## 1. Phase 1 — Player ratings (Davies 66–67, Vallejo 69)

### Root cause
**The game was running a stale world, not a wrong formula.** Evidence, from the user's own files:

| | FM24 source (`world.db`) | Installed world (**revision 4**, what the user saw) | Revision 6 (F2 code) |
|---|---|---|---|
| Alphonso Davies (person 9453) | CA 150 / PA 168 | **66** / 72 | **83** / 89 |
| Jesús Vallejo Lázaro (person 29715) | CA 127 / PA 148 | **69** / 76 | **76** / 82 |

`fm24-install.json` records `revision: 4`; the code is at 6. Revision 4 derived a level from the mean
of converted attributes, which squeezed the whole hierarchy into 55–70 (a world-class full-back with
90 pace and acceleration came out at 66, above-average centre-backs at 69). The F2 change replaced
that with a monotone curve on CA and bumped the revision so an installed world is *offered* for
regeneration — but the user's game was never re-imported.

### What was verified
Running the revision-6 importer on the real source (into a scratch directory), across all 10,045
imported players:

* correlation of rating with CA: **0.914 → 0.998**;
* pairs with a CA gap of 20 or more where the worse player is rated higher: **2,256 → 0** of
  130,719 sampled;
* top of the world: Kane 96, Mbappé 95, Courtois 95, Haaland 95 (CA 195/191/190/190);
* the stored rating survives the game's own recomputation from attributes (`natural_ovr` within 1
  for every FM24 player), so it does not drift on the first training or ageing refresh.

### What changed
No change to the rating formula was needed. Added `season-harness/tests/fm24_ratings.rs` (three
tests, read the player's own world through `APFM_WORLD_JSON`, nothing from FM24 in the repository):
Davies rated at least 4 above Vallejo; stored rating equals the attribute-derived one; the
population spans the scale. **Seen failing** on the installed revision-4 world (`fm-p-9453 is rated
66, fm-p-29715 69`) and passing on a revision-6 import. The older ignored
`fm24-adapter/tests/real_world_hierarchy.rs` was also run against the real source and passes.

### What the user must do, and what was deliberately not done
* **Regenerate the world** from the world screen (it reports `outdated`). That affects *new*
  careers.
* **Existing careers keep their stored players** (Davies stays at 66 in a career started on revision
  4). Nothing re-rates or overwrites a save. An explicit, opt-in re-rating of an existing save
  would be a separate piece of work (§9).
* No rating was hardcoded; no player was forced above another.

### Source data used
`person.id`, `player.ca`, `player.pa`, `player_attributes`, `player_position` of the user's FM24
`world.db`. No row is committed; the test names two person ids and asserts only an ordering.

---

## 2. Phase 2 — Transfer negotiations

### The six stages you asked for, against what exists now

| # | Stage | State |
|---|---|---|
| 1 | Club offer | existed (`make_transfer_bid_with_terms`), unchanged |
| 2 | Club response / counter-offer | existed (rounds, counter floor, break-off, cool-down), unchanged |
| 3 | Player contract proposal | existed from F1 (`PlayerTalks`: nothing moves until the contract is put to him). **Now reachable from every entry point** (below) |
| 4 | Player accepts / refuses / counters | existed (appeal score; `player_demand`) |
| 5 | **Administrative processing** | **new**: `transfers/processing.rs` |
| 6 | Registration and effective date | existed (`PendingRegistration`, daily processor); now used with the window open too |

### Defects and causes

| Observed | Cause | Fix |
|---|---|---|
| "The modal only lets me change the fee" | `TransferBidModal` took `bidTerms` / `onBidTermsChange` as **optional** props and hid the whole terms section without them. Only the Transfers tab passed them; the Players list, Scouting and Player profile did not. | Props are **required** now (a caller that forgets them no longer compiles) and the three callers pass them. Test: opening a bid from the Players list shows wage, length, bonus and sell-on (red before, green after). |
| "Sales complete immediately after accepting" and "paid transfers resolve too quickly" | With the window open, `transfer_registration_date` returned *today*, so every bid and every accepted incoming offer registered in the same call. | `deal_registration_date` is the one place that decides it (below). |
| Free-agent signings "too quick" | They already take 1–3 game days from Run C (`FreeAgentTalk`). **Not changed.** There is still no separate administrative step after the player says yes; see §9. |

### The processing delay
* Window **closed**: the opening date, as before.
* Window **open**: today plus a deterministic number of days — fee under 500 000: 0–1; 500 000 to
  5 000 000: 1–2; above: 2–3; **+1** if the clubs are in different countries; **+1** for instalments
  or a sell-on clause. The pick inside the range is a stable hash of player, both clubs and the day,
  so reloading or fast-forwarding gives the same date.
* **Never past the closing day**; on deadline day there is nothing to wait for and the deal
  completes at once.
* Used by the manager's purchases, the manager's sales (accepting an incoming offer), the preview
  that promises a date, and — because the assistant bids through the same path — delegated
  recruitment.
* Until the date the player is **still at his club and nothing has been paid**. On the day the
  existing processor re-checks funds, loan conflicts and the player's consent, and withdraws the
  deal with a reason if any now fails.
* A window that does not state how long it has left (only hand-built state: an open window always
  does) adds no wait. This is what keeps the 2,700 existing fixtures valid; it is a documented
  convention, not a rule of play.

### Evidence
`ofm_core/tests/transfer_processing_tests.rs` (9 scenarios, red before the change for the two core
ones), `db/tests/pending_transfer_reload_tests.rs` (open-window deal saved mid-processing, reloaded,
registers identically), 4 unit tests on the pure delay function. Scenarios: agreed deal waits and
leaves the player at his club with cash unmoved; moves on the date and not before, exactly once;
deterministic; capped at the closing day; deadline-day immediate; a sale does not move him that
day; a sale falls through if the buyer can no longer pay; a player with a sale pending cannot be sold
twice.

### A defect this exposed, and fixed
While a delegated purchase waited for its date the squad had not changed, so the assistant's review
still called the position weak and **he bought a second defender**. `the_assistant_signs_a_suitable_player_within_the_limits_he_was_given`
caught it. Fix: a group with a purchase pending registration is not a need
(`groups_with_a_purchase_pending`). The assistant's log now says `recruitScheduled`, which is true,
rather than `recruitSigned`.

### Also changed
* MCP `transfer_make_bid` now returns `registration_date` and states what it means: **`accepted`
  means agreed, not completed.** Without it an agent would read "accepted" as "he has moved". Test in
  `mcp-results`.
* Wording that said "when the window opens" is neutral in all 12 locales.

### Not done (stated plainly)
* **Contract promises** (playing time, role) are not supported by the backend or the accounting; none
  was added. Signing bonus, instalments, sell-on and the stepping-stone promise were already
  supported (Run E) and are now editable at every entry point.
* AI-to-AI transfers still complete instantly; only deals the user's club is part of are delayed.
* The player's side of the negotiation has no "round" memory beyond the existing demand/counter.

---

## 3. Phase 3 — Loans

### Audit of what existed
Proposal, duration, wage contribution, purchase option, owner response with counters, pending
registration, development reports and return to the parent club (`process_loan_returns`) all
existed. Gaps found:

| Gap | Done |
|---|---|
| No player consent anywhere in the loan path | **Added** `transfers/loans/consent.rs`: a fringe player welcomes a loan; a regular accepts a drop of up to 150 reputation points; a first-eleven player accepts at most 50; anyone unhappy (morale ≤ 45) or transfer-listed goes. Deterministic, explainable, applied when the owner has agreed and **before** anything is marked agreed, on all three paths (manager's offer, accepting an incoming offer, countering). Refusal is `be.error.transfers.playerRefusesLoan` (12 locales). |
| Loans started the day they were agreed | **Same administration as a permanent move**: a plain loan 0–1 days, one with a purchase option 1–2 (a contract to draw up). Start date and end date follow. |
| A player who cannot be approached produced a bare error after submit, without a date | `DealRestrictionNotice` in both the bid and the loan form (so every entry point gets it): names where he is on loan and **until when**, or that a move is waiting for registration and **on which day**. The date comes only from the player's record (`active_loan.end_date`, `registration_date`, a pending loan's `start_date`); with none stored it says so without one. Submit is disabled while a restriction holds. |
| Loan fee | **Not modelled** by the domain or the accounting. Not invented. A loan costs the borrower his wage share and nothing else. |

### Evidence
`ofm_core/tests/loan_negotiation_tests.rs` (8 scenarios): agreed loan waits for its start date and
leaves the player at his club; starts on its date owned by the parent with the agreed end date;
a player on loan elsewhere is refused and his availability is the loan's end date on his record;
returns to his club at the end date; a first-eleven player refuses a much smaller club; the owner
rejects too small a wage share; a borrower whose wage policy cannot carry the share is refused before
anything is agreed; a pending loan blocks a second proposal. 5 unit tests on the consent rule, 5 on the
restriction helper, 3 + 1 component tests.

### Not done
* AI-to-AI loans are unchanged (no consent, no delay).
* Overlap, expiry and season-transition coverage beyond the above relies on the existing tests
  (`a_contract_end_loan_returns_then_releases_the_player`, and `a_loan_runs_its_course_and_the_player_goes_home`
  and `a_loan_in_progress_survives_a_save_and_a_reload` on the real FM24 world; see §7).

---

## 4. Phase 4 — M12 match integration (a first vertical slice, not the whole job)

### Defects and causes

| Observed | Cause |
|---|---|
| Scoreboard `FM-C-1072` | The header printed `replay.home_team`, which is the id the engine carries unchanged. |
| Players too small | radius `max(5, scale)` ≈ 7 px. |
| No goals visible | `drawPitch` drew no goals, boxes or arcs at all. |
| Raw English event labels | Anything not in a 4-item list was printed as the engine spelled it. |
| Isolated from the match interface | The viewer was a bare `<main>` outside `MatchScreenLayout`. |

### What now exists
* **One match, one clock, four synchronized views** (`ReplayViews`): 2D pitch, player condition,
  statistics, line-ups and substitutions. They all read the same stored replay at the same `time`;
  switching never moves the clock, the score or the players (`changing view does not change the
  clock, the score or the match`). Nothing is re-simulated by viewing.
* The viewer sits in the standard match shell (`MatchScreenLayout`, theme toggle, header).
* **Club names** from the game's clubs (exact or case-insensitive id), a translated **Home / Away**
  fallback when a name is missing — never the id. Players the game cannot name are "a player", never
  an id. Stored (read-only) replays are named from the game state.
* **Pitch**: penalty and goal areas, goals with nets, larger numbered players, a ring on the player on
  the ball, the goal mouth highlighted when one is scored (from the goal event's real position), and
  a goal announcement with scorer and minute.
* **Feed**: built from the real event vocabulary (`replayDerived.buildFeed`): goal, own goal, shot
  saved/wide/blocked/woodwork, cards, substitution, injury, corner, foul, offside, kick-off, half time,
  full time — in all 12 locales. The engine's other ~20 kinds (`ball_control`, `pass_completed`,
  `state_change`…) are dropped, not printed.
* **Statistics** are counts of recorded events and frames. **At full time they equal the engine's own
  totals**, asserted against a real M12 replay (shots 35/37, on target 29/24, corners 6/2, fouls 7/1,
  offsides 0/1, possession 52.8%). The live number and the final number are one thing.
* **Condition** is the engine's recorded per-player value (`frames[].extras`), switched on with
  `m11.replay_interval_ms = 1000`. It is a recorder: against the real engine the whole replay document
  of the same fixture is **identical** with it off or on apart from the added samples
  (`recording_player_condition_does_not_change_the_match`). A replay recorded before this change has
  none and the view says so.
* **Replay-only versus live**: the line-up view states that it is a replay and that tactics and
  substitutions are decided in a live match; nothing in it can change the result.

### Real M12 validation
* `scripts/apfm/run-m12-tests.sh` against `~/apfm-match-lab`: **4 + 32 + 8 tests pass** (`spatial_runner`,
  `spatial_*`, `commands::spatial_match`), and it refuses a stub or a zero-test selection.
* A genuine replay (5,400 s, 10,801 frames, 4,045 events, 30-player roster) was exported with
  `export_a_real_replay_for_the_viewer_validation` and the viewer's data layer run on it
  (`replayDerived.real.test.ts`, skipped unless `APFM_REAL_REPLAY` is set): totals reproduced,
  every goal in the feed, a condition for every player on the pitch falling over the match, eleven a
  side at kick-off.
* The drawing code was run on two real frames (kick-off phase and the first goal) and rendered in
  headless Chrome; the screenshot was inspected (numbered players ~25 px across, goals and nets,
  penalty areas, highlighted goal mouth, ball-holder ring). This is the only visual check; **the app
  itself was not launched** (§8).

### Not done — remaining work
1. **Replay size.** The real replay document is **78 MB per match** (≈ 68 MB before this run; the
   condition samples add ≈ 10 MB, 13%). It is stored per match and parsed whole in the webview. This is
   the biggest open problem of the match path and predates this run. Recommended: a compact/sparse
   frame encoding or compression, and not shipping 500 ms frames of 30 players.
2. **Live decisions in M12 matches.** "Watch in 2D" pre-simulates the whole match and plays it back; a
   manager cannot change tactics or make a substitution at a legal stoppage during an M12 match.
   M12 has a live command session (`LIVE_COMMAND_CONTRACT.md`) that the app does not drive. The classic
   live engine remains the only place those decisions exist. This was **not** attempted.
3. For a stored replay the home/away split falls back to "roster in half", which is wrong when the two
   benches differ in size (the just-prepared replay uses the snapshot and is right). The engine does not
   emit a side per roster entry.
4. Not verified in the running desktop app (§8).

---

## 5. Phase 5 — Delegation and daily-management audit

Measured, not just read: `season-harness/tests/measure_delegation_modes.rs` plays the **same club,
world and seed for 120 days** under each mode (`-- --nocapture` prints the table).

| Mode | Executed by the assistant | Left to the manager | Open proposals | Eleven changed | Inbox items with an unresolved action |
|---|---|---|---|---|---|
| Manual | nothing | 0 | 0 | 0 | 87 |
| Approval | Lineup 18, MatchPreparation 1, Scouting 29 | 0 | 0 | 18 | 119 |
| Full | Lineup 18, MatchPreparation 1, Scouting 29 | 0 | 0 | 18 | 119 |

### Findings
1. **Manual really does nothing. Approval and Full are indistinguishable over 120 days.** The
   difference between them is only whether *spending* decisions (renewals, recruitment, staff hiring
   above the limits) are proposed or executed. None was reached in this period, so the user's
   observation that "Full does not seem to manage everyday responsibilities" is **true of what Full is**:
   it automates line-ups, rest/preparation and scouting, and nothing else without spending.
2. **What runs each day** (`run_after_day`, with periods): Lineup (1 d), MatchPreparation (1 d, acts only
   the day before a match), Training (1 d, before the day's session), Scouting (3 d), PlayerRecruitment
   (3 d), Youth / StaffRecruitment / StaffRenewals / PlayerRenewals (7 d). 120 days produced executed
   entries only for Lineup, MatchPreparation and Scouting.
3. **Informational versus decision.** Of the 119 inbox items with an unresolved action under Full,
   about 60 are informational (MatchPreview 10, MatchResult 10, Media 5, ScoutReport 29, LeagueInfo 2,
   System 1, Training 1): an *acknowledge* button, no decision. The genuine decisions nobody answers
   are **Transfer 27** (incoming offers for the manager's players), **PlayerMorale 28** (player
   conversations), **Injury 5**, **Contract 1**.
4. **Delegation adds inbox noise**: +29 scout reports versus Manual, all acknowledge-only.
5. **No actor exists for**: answering incoming offers for the manager's own players (the assistant
   deliberately never sells or lists — tested), player conversations/morale, injury decisions, the
   in-match tactical decisions. The match-day eleven is chosen; match-day tactics are not.
6. The authorisation machinery itself (ceilings, the safe zero default, proposals instead of spending,
   save/reload and rollover) is exercised by `delegated_career`, `delegated_recruitment` and
   `fast_forward` in the harness, which pass.

Nothing was added to delegation in this run, as asked.

---

## 6. Files modified

(See `git diff --stat b2cb184b..HEAD`; 39 files plus 13 locale/exception files, ≈ 3,400 lines added.)

* **Backend**: `ofm_core/src/transfers/{processing,loans/consent}.rs` (new), `bids.rs`, `loans.rs`,
  `loans/incoming.rs`, `mod.rs`, `consts.rs`, `staff_system/delegation_recruitment.rs`,
  `spatial_runner.rs`, `mcp-results/src/{lib,transfers}.rs`, `mcp_server/tools_impl/transfers.rs`.
* **Frontend**: `components/transfers/{TransferBidModal,LoanOfferModal,DealRestrictionNotice,dealRestriction}`,
  `players/PlayersListTab`, `scouting/ScoutingTab`, `playerProfile/PlayerProfile`,
  `match/{M12ReplayViewer,ReplayViews,ReplayPlaybackBar,ReplayPanels,ReplayEventFeed,replayDerived,replayNames,replayPitch,m12Replay}`,
  `pages/MatchSimulation`.
* **Tests (new)**: `season-harness/tests/{fm24_ratings,measure_delegation_modes}.rs`,
  `ofm_core/tests/{transfer_processing_tests,loan_negotiation_tests}.rs`, plus the unit tests named above.
* **i18n**: 12 locales (+ `INTENTIONAL_SAME.json`): the refusal message, the restriction notice, the
  neutral registration wording, and 47 keys for the replay views (the superseded `replayEventType`
  strings were removed).
* **Docs**: this file; `docs/MCP_SERVER.md` (`accepted` ≠ completed).

---

## 7. Tests executed — exact results

All commands from `~/apfm-migration/openfootmanager`. `APFM_WORLD_JSON` was a **revision-6 world
imported from the user's real `world.db` into a scratch directory**, so the real-FM24 tests ran for
real (without it they return at once and prove nothing).

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`, default and `--features mcp` | clean, both |
| `cargo test --workspace --no-fail-fast` with the real world | **109 targets: 4,299 passed, 4 failed**, 56 ignored; after fixing two of the four (below), `fm24_loans` re-run: 3 passed, 1 failed, so **4,301 passed, 2 failed**; the remaining two were then investigated and fixed ("The two pre-existing failures", below), and `fm24_loans` and `fm24_global_market` re-run: 4 / 4 and 3 / 3 passed on both the revision-6 and the installed revision-4 world. The whole workspace was not re-run after that follow-up |
| `cargo test --workspace --features mcp --no-fail-fast` (no world set, so real-world tests were vacuous) | **109 targets: 4,389 passed, 0 failed**, 56 ignored |
| `scripts/apfm/run-m12-tests.sh` against the real `~/apfm-match-lab` | `spatial_runner` 5 passed, `spatial_*` 33 passed, `commands::spatial_match` 8 passed ("M12-dependent tests passed"); the 1 ignored in each of the first two is the replay-export helper |
| Real-engine proof that recording condition does not change the match | `recording_player_condition_does_not_change_the_match` passes: whole replay document identical apart from `frames[].extras` |
| `npm run format:check`, `npm run lint`, `npm run knip` | clean (Biome 2.5.5, from `node_modules/.bin`) |
| `npm test` | **327 files / 2,887 tests passed**, 1 file (4 tests) skipped: the real-replay test, which needs `APFM_REAL_REPLAY` |
| Same real-replay test with a genuine 78 MB M12 replay | 4 / 4 passed |
| `npm run build` (`tsc && vite build`) | passes |
| i18n gates (`localeCoverage`, `frontendKeyCoverage`) | pass; 7 identical-to-English strings that are the real football word (`corner`, `possession`, `Fouls`, `kick-off`, `offside`) are listed in `INTENTIONAL_SAME.json` |

### The 4 failures of the first workspace run, attributed

The same two test files were run on the commit **before** my changes (`b2cb184b`, isolated worktree and
build directory, same world):

| Test | At `b2cb184b` | With this run | Verdict |
|---|---|---|---|
| `fm24_loans::an_ai_club_refuses_a_stingy_loan_and_accepts_a_fair_one` | passes | failed | **mine, fixed** |
| `fm24_loans::a_loan_runs_its_course_and_the_player_goes_home` | passes | failed | **mine, fixed** |
| `fm24_loans::ai_clubs_offer_for_a_loan_listed_player_and_he_comes_back` | **fails** | fails | **pre-existing**: `be.error.squadFloor.wouldLeaveShort?group=Midfielder` when the manager accepts an AI loan offer |
| `fm24_global_market::clubs_outside_the_players_league_buy_players` | **fails** | fails | **pre-existing**: "289 of 506 transfers crossed a border: expected some, but not most" |

The two I caused assumed a loan starts the day it is agreed, which is the behaviour this run changes on
purpose. They now run the registration the turn runs on the loan's start date
(`start_the_agreed_loan`) and their assertions are unchanged. One more test of the same family,
`delegated_recruitment::a_delegated_career_recruits_within_its_ceiling_and_remembers_across_reloads`,
counted `recruitSigned` log entries and failed for the same reason; it now counts players who actually
arrived at the club by day 60, a stronger check.

### The two pre-existing failures: investigated, both were test defects

They failed identically at `b2cb184b` and also on the installed revision-4 world, so ratings were not the
cause (the earlier guess in this report that they were was wrong). Neither was a game defect.

**`fm24_global_market::clubs_outside_the_players_league_buy_players`** (289 of 506 "transfers" crossed a
border). The observer counted every change of club by a player who was not on loan that day. That includes a
loan **ending** (the player goes home, usually across a border: 149 of 220) and free signings (15). Counting
only the days that wrote a `PermanentTransfer` into the player's movement ledger gives **271 transfers, 130
across a border (48 %)**, and the count matches the harness's own deal log (271). The test now counts that.
Measured on the game: 47-49 % over seeds 1, 2, 3, 21 and 99 (one scoped season); 41 % in season 1 and 39 % over
five seasons with the whole world active, against the 37 % recorded in `APFM_MARKET_FINAL_VALIDATION.md`. The
market has not drifted. The test's bar ("most from their own country", under 50 %) passes by about one
point, so a later market change that nudges the share up will fail it, which is the point of the bar, but it
would be a calibration decision rather than a regression.

**`fm24_loans::ai_clubs_offer_for_a_loan_listed_player_and_he_comes_back`.** The test lent the user's
player with the largest gap between potential and rating, whoever he was. Three things were wrong with that
choice, each hiding behind the previous one:

1. On the revision-6 world he was a midfielder the club could not spare, and `squad_floor` refused
   (`wouldLeaveShort?group=Midfielder`). The rule is right; the fixture now picks a player the club can
   spare.
2. On the revision-4 world he was a first-team player and he refused a loan to a smaller club
   (`playerRefusesLoan`): the F2 consent rule working. The fixture now lends a fringe prospect (outside the
   first eleven, market value under the 1.5 M "regular" line).
3. Both worlds: the loan waits for its start date, which the test did not expect. It now plays whole days
   until the loan starts, and checks the borrower's wage bill against the same game without the loan (the
   bill read days earlier had moved by 1,873 with the club's other business).

A first idea, treating a loan-listed player as willing to go, was tried and **rejected**: the manager can only
ask for, and AI clubs can only bid for, loan-listed players, so it would have made the consent rule
unreachable. The rule is unchanged.

Not touched: `start_the_agreed_loan` (used by two other loan tests) still moves the clock to the start date
without playing the days in between. Those tests pass, but the same shortcut made the invariant
`no-stranded-fixture` fail in the third test, so it is a fragile helper; converting it is a follow-up.

### Frontend tests added or changed by this run
`PlayersListTab` bid-modal terms (red then green), `TransferBidModal`/`LoanOfferModal` fixtures,
`dealRestriction` (5), `DealRestrictionNotice` (3), `LoanOfferModal` restriction (1), `replayNames` (3),
`replayDerived` (8 + 4 real-data), `M12ReplayViews` (9), `M12ReplayViewer` (12, adapted: shell, wording
and the removal of the engine's raw labels are the intended changes).

---

## 8. Known limitations and honesty notes

* **The desktop app was not launched.** The frontend is verified by Testing Library tests, a real-data
  derivation test, a production build, and one headless-browser render of the canvas code on real
  frames. A person using the real window has not been simulated.
* **Not every new test was seen failing first.** Seen failing for the predicted reason: the
  Davies/Vallejo test on the revision-4 world, the Players-list modal test, the two core
  transfer-processing scenarios, the assistant double-purchase. Written together with the code and
  therefore **not** red-first: the loan-consent unit tests, `dealRestriction`, `replayDerived` and
  `replayNames`. Their value rests on the scenario tests and the real-data test above.
* The project's reviewer agents (`ofm-architecture-reviewer`, `i18n-auditor`,
  `ui-accessibility-reviewer`, `ofm-dedup-reviewer`, `ofm-test-reviewer`) were **not run**, so none of
  their findings are claimed absent. The equivalent mechanical gates were run (§7).
* A formatting slip was found and fixed: my first `format` calls were a silent no-op (there is no such
  script and I discarded the output), so earlier frontend commits of this run were unformatted until
  `biome format --write` was run with the real binary; `format:check` passes at HEAD.
* The "no wait when the window does not state its remaining days" convention (§2) exists so that
  hand-built fixtures stay valid. Real play always has the value.
* Existing careers started on revision 4 keep their old ratings.

---

## 9. Recommended next run

1. **Replay storage** (78 MB per match): a compact frame encoding / compression and a lower sampling
   rate; measure against real SQLite size.
2. **Live M12**: drive M12's live command session so tactics and substitutions at legal stoppages are
   real decisions, and decide how a replay-only view and a live view share one component.
3. **Delegation, dedicated run**: an actor for incoming offers on the manager's own players (reject the
   clearly low, escalate the rest), for player conversations, and for match-day tactics; make Approval
   and Full differ where the manager can see it; collapse informational inbox items so they are not
   "decisions".
4. **Opt-in re-rating of an existing save** from a regenerated world, with a diff the manager confirms.
5. **Free-agent signing**: an administrative step after the player says yes, consistent with paid
   transfers; AI-to-AI deals and loans through the same processing so the world is consistent.
6. A loan **fee** in the domain and the accounting, if the design wants one.
7. Replace the `start_the_agreed_loan` clock shortcut in the two remaining loan tests with played days.
   (The two pre-existing real-world failures were investigated and fixed after the first report; see §7.)
