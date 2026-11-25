# ALPHA6.2 — M12 tactical interface, simulation reports and frontend state consistency

Written for: the owner and the independent reviewer of ALPHA6.2.

**Verdict: see section L.** Everything below says what was measured, what was changed, what was
only measured, and what was not done. Where a number comes from a run it names the run.

## A. Baseline and SHAs

* Required baseline `a841051c23f1449c6c9951b7a7be64802138281f` (confirmed: `git rev-parse HEAD`, clean
  tree on `apfm/main`).
* Final SHA: the last line of this file, written at push time. Commits of this mission, in order:

| Commit | What |
|---|---|
| `bd3a1954` `fix(state): order full-game answers by an authoritative per-career revision` | section B |
| `86fef7b5` `feat(match): Formation & Substitutions tab for the interactive M12 match` | sections C–E |
| `2e2beb0a` `feat(fast-forward): a structured simulation report from the game's own records` | section F |
| `docs(alpha6.2): …` (this document) | all |

* The owner's database `~/apfootballmanager/data/world.db`: SHA-256
  `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` at the start; re-read at the end
  (section M). Source-backed runs used `~/apfm-scratch/alpha6-world/apfm-fm24-2026.json` (read-only) and
  `~/apfm-scratch/world-copy.db`, a scratch **copy** of `world.db`.
* Not touched: the match-engine repository (the pinned `apfm-match-lab` revision is unchanged), the
  transfer-market rules, squad registration, the fatigue formula, the calendar scheduler.

## B. Out-of-order snapshots (step 1)

### B.1 The defect, reproduced

ALPHA6.1 §1.2 named it as residual: the backend took the answer's game under the commit lock, but nothing
ordered two answers on the screen. Reproduced before any fix, in vitest:
`keeps_the_later_commit_when_two_transfer_answers_arrive_reversed` — two successful transfer commands,
the backend commits A then B, the answers reach the screen as B then A, and the screen ended on A's cash
(600 instead of 700). Three more tests were red for the right reason (an older snapshot of the same
career replaced a newer one; a late answer of a replaced career replaced the new career). The Rust side
was reproduced as compile errors (no `state_revision` existed) and then as red assertions.

### B.2 The rule

* `Game::state_revision` (`#[serde(default)]`, not saved, 0 for a game never installed) counts the writes of
  the installed career. It travels with every game the backend sends, next to `career_token`.
* `StateManager::update_game` advances it by one **before** its closure runs, under the game lock, so a
  snapshot taken inside the closure (`ui_clone`) carries the revision of the state it shows, and a closure
  that swaps the whole `Game` cannot rewind it. A write that finds nothing to do still advances it; only
  the order matters.
* `update_game_when` is the same for a change that may be refused or replayed: when the closure's answer
  is a refusal or a replay, the revision is put back. `run_transaction` and `advance_to_next_season_internal`
  use it, which keeps the ALPHA6 guarantee that **a refusal leaves the game byte for byte as it was**
  (six ALPHA6 tests assert it by serialised comparison; they stayed green once the rule existed, and
  they were red the first time the revision advanced on a refusal).
* `set_game` assigns the revision itself: the same career continues its count (+1, even when the argument
  is an older clone), a different career starts at 1. `install_career_for_screen` returns the screen's copy
  with the installed revision.
* Screen (`store/gameSnapshotOrder.ts`, applied in `gameStore.setGameState`): same career and a lower
  revision → dropped; same revision → accepted (the screen's own edit of that state); a lower
  `career_token` → dropped (tokens are never reused in a process and increase with each career installed,
  so a lower one is a career that was replaced); a higher token → accepted whatever its revision; a game
  with no stamp (an older backend, a fixture) → accepted.
* **Every path that puts a whole game into the store passes this one gate**: transaction answers
  (`onGameUpdate`), `get_active_game` refreshes, the `game-state-changed` event MCP-driven updates raise,
  time advancement, fast-forward, career hydration and the menu's resume. No production code writes
  `gameState` any other way (searched: `useGameStore.setState` is test-only).

### B.3 What this does not claim

* Commands that return only a **slice** of the game (the inbox's `setMessages`) are not stamped, so
  they are not ordered against a full snapshot.
* After `clearGame` there is nothing on screen to compare with, so a late answer of the career just left
  is not recognised by revision; the existing `careerEpoch` guards and `submitTransaction`'s career check
  cover the dialogs that matter. The revision protocol is **not** claimed to solve every possible race.
* The 200-odd `set_game` call sites in tests apart, production has five (`career.rs`, three in
  `interactive_season.rs`, a dev benchmark); the rest of the backend writes through `update_game`.
  Paths that return a locally modified copy after `set_game` carry the revision they were cloned at, which
  is conservative: it can only make the screen drop an answer that was already older.
* A snapshot is still a full `Game` clone per transaction (4.8 ms on the 16,609-player world, ALPHA6); the
  revision adds one integer. The transaction ledger holds no snapshot.

### B.4 Tests

Rust (`ofm_core`): `every_update_advances_the_revision_and_a_read_does_not`,
`a_new_career_starts_at_revision_one`, `replacing_the_same_career_with_an_older_clone_does_not_rewind_the_revision`,
`a_closure_that_replaces_the_game_cannot_rewind_the_revision`,
`a_replacement_career_restarts_the_count_under_its_own_token`,
`two_commits_answered_out_of_order_are_ordered_by_the_revision_they_carry` (**both transactions are
committed** and the later commit's revision is the higher), `a_replayed_commit_reads_the_current_revision`,
`concurrent_commits_never_share_a_revision` (8 threads × 25 transactions, 200 distinct revisions),
`a_refusal_and_a_replay_leave_the_revision_where_it_was`. Frontend (`gameStateRevision.test.ts`, 7 tests):
older dropped, newer accepted, same accepted, two careers never compared, replaced career dropped, no stamp
accepted, and the reversed-order scenario end to end through `submitTransferCommand`.

Mutation checks for this system are in section K.6 (every one killed).

## C. M12 Formation & Substitutions panel (step 2)

### C.1 What was built

A tab **"Formation & Substitutions"** beside **"Live match"** on `InteractiveMatchScreen` (a `Tabs`/`TabPanel`
pair; shown whenever the match has a manager's side). The existing list controls (`TacticsPanel`,
`SubstitutionPanel`, both lineups) stay in the right-hand column in both modes: the new panel is
alongside them, not instead of them. The panel is mounted only while its tab is selected, so a hidden
copy of the lineup never puts every name on the screen twice, and "Back to the live match" (header
button, plus the first tab) returns to the 2D pitch.

* **The pitch.** The manager's current formation on a tactical pitch, eleven (or ten) tokens, each a
  `role="button"` named after the player, showing shirt number, surname, the slot's role and condition,
  with `+` (injured) and `▼` (substitution queued) markers backed by words in an `sr-only` description.
  It reuses `FormationPitch` (the SVG, selection, click/Enter/Space wiring, focus handling) through one
  new optional `slots` prop; a small tested adapter `toPitchPlayer` gives it the player type it expects.
* **Where a player stands is the engine's, not a guess.** `InteractiveView` gained `formation_shapes`
  (`ofm_core::interactive_match::plan::formation_shapes()` reads the engine's own `FormationPreset::slots()`:
  role, depth, lateral for all six supported formations). A player is matched to a slot of his own role
  (`role` is the engine's slot role), in the engine's order for equal roles; a team reduced to ten leaves
  the vacated slot empty; a player whose role fits no slot, or a formation with no shape, is still drawn.
  This matters: 3-5-2 and 5-3-2 put their wing-backs at **midfield depth**, which no mapping from the role
  name would give. Two Rust tests pin the contract (`the_view_carries_the_engines_own_slots_for_every_supported_formation`,
  `the_roles_on_the_pitch_fill_the_slots_of_the_formation_shape`).
* **No overlap by construction.** Engine slots are metres apart and a token is not: `keepApart` nudges two
  tokens that would touch along the axis with the smaller overlap, deterministically (it was needed: 4-5-1's
  deep midfielder sat 6 m behind two central ones). A vitest geometry test checks all six formations at the
  pitch's real size, and the real browser measurement (section D) confirms it.
* **Selecting a player** opens the detail block: shirt number, name, slot role (and natural position when
  different), condition bar and %, minutes, yellow cards, and "waiting for a stoppage" / "injured".
* **Substitutes** are listed beside the pitch (one column, name never cut by a second column): enabled
  only once a player to take off is chosen, never for a substitute already queued to come on.
* **Take off → bring on → confirm**: select a token (outgoing), select a substitute (incoming), press
  "Substitute X for Y" (the button names the pair). A "Clear selection" button undoes the selection.
* **Queued substitutions** ("waiting for a stoppage") are listed with a Cancel button each (the existing
  `CancelSubstitution` order); the allowance line says used / max / waiting / windows left.
* **Substituted, dismissed and injured** are told apart in "Left the match": **Sent off** (danger badge),
  **Injured** (the engine's `injured` flag on a player who is in `used`), **Substituted**. None of them is
  a button, so none can be selected.
* **Formation change** through the same path as the list control: a select of the supported
  formations (the engine's own list), "Change formation" disabled until it differs, one `Tactics` order
  carrying only the formation.

### C.2 What the panel does not do

It holds only the manager's *selection*. The lineup, bench, queue and allowance it shows are always the
engine's current `InteractiveView`; confirming an order does not move anyone on screen. A selection of a
player the engine has since taken off or queued is dropped on the next view (derived, not effect-driven).
The panel does not recompute any football rule: whether a goalkeeper may be replaced by an outfielder,
whether a window is left, whether a formation change is legal with an injured player on the pitch, are
the engine's refusals, shown in the screen's existing words (`match.interactive.rejection.*`). The only
local arithmetic is the allowance hint (`remaining − queued`, the same expression the assistant's
`allowance()` uses) that closes the bench early; the engine still decides.

## D. Measured browser layout (step 2)

Measured in **real Google Chrome (headless)** on the **real `InteractiveMatchScreen`**: the actual
`FormationSubstitutionsPanel`, `FormationPitch`, tabs, list controls and stylesheet. Only the Tauri IPC is
replaced, and it answers with **the engine's own views**: one real M12 session per supported formation
(generated by a real match: start, send the real `Tactics` order, advance 40 s, read the real
`InteractiveView`). Probe, generator command, PNGs and `results.json` are archived in
`docs/appendix/formation-panel-probe/` (README says how to re-run; the probe files were deleted from the tree).

**72 runs** after the layout fixes below: window widths **960 (the narrowest supported window), 1280, 1440, 1728**
× 6 formations (4-4-2, 4-3-3, 4-2-3-1, 3-5-2, 5-3-2, 4-5-1) × {normal 11 players; "busy": ten players,
a queued substitution, a substituted and an injured player listed, an injured player on the pitch;
"busy" with 45-character surnames}.

| Check | Result (all 72 runs) |
|---|---|
| tokens drawn | 11 (normal) / 10 (busy), in every run |
| overlapping token footprints | **0** (the pitch is 416 × 624 px at every width; tokens are 72 px wide) |
| tokens outside the pitch | **0** |
| controls whose centre is covered by another element, or outside the viewport | **0** (17–19 controls per run, scrolled into view and hit-tested) |
| horizontal document overflow / elements wider than the viewport | **0** |
| keyboard-focused token shows a ring in the real stylesheet | yes, every run |
| names cut with an ellipsis | 0 for real names; 10 of 10 for 45-character surnames (cut, not overlapping; the full name is the token's accessible name and the detail heading) |

The viewport is 15 px narrower than the window (a scrollbar), so "1280" was measured at 1265, "1728" at 1713.

**Defects the screenshots found that the numbers did not** (each fixed, then the whole matrix re-run; the
table above is the last run): wing-backs of 3-5-2/5-3-2 were drawn on the touchline (tokens are now clamped
inside the drawn markings, which a stricter unit test pins); in two columns the bench cut a name to "hom…"
(one column now); the selected token was not visibly different in the dark theme (a ring, an underline and
`aria-pressed`, added to `FormationPitch` for every user of it); the header button wrapped under the title
(the card header's `action`); after the accessibility review gave names and condition their own dark pill, the
names were cut to "home-…" (the token grew from 64 to 72 px); and a container-query threshold set too high
stacked the controls under the pitch at 1280 (it now reads 44 rem of the card's own width). Four of the six
rounds of measurement showed **0 overlaps while the screenshot still showed a defect**: overlap alone is not
proof of a usable layout, which is why every round's screenshot was read.

**Not measured:** the macOS Tauri webview (**the Tauri GUI was not operated**); light theme, high-contrast
and compact density (design tokens only, no visual check); a real Tab-key walk (headless `--dump-dom`
cannot send keys: Enter/Space and focus return are tested in jsdom, the ring is measured); `ui_scale` ≠ 1
(sizes are rem-based so the proportions hold, not measured); window heights other than 813 px of viewport.

## E. Substitution and command correctness

* All substitutions are `UserOrder::Substitute { player_off_id, player_on_id }`, cancellations are
  `UserOrder::CancelSubstitution { player_off_id }`, formation changes are `UserOrder::Tactics { formation }`:
  the panel only builds the order; `useLiveMatch.giveOrder` (unchanged) adds the command id
  (`session:salt:counter`, unique to the screen), the **expected revision** of the view it was given
  against, refuses to send unless the picture has caught up with the engine (`canOrder`), and shows the
  engine's rejection through the existing notice. The panel passes `canOrder && !finished`.
* Tested through the screen with the fed-back engine fake (`InteractiveMatchScreen.formationPanel.test.tsx`,
  7 tests): the order reaches the engine against the revision it had; the queue shows it only once the engine
  has queued it; cancel sends the engine's cancel order; a **stale revision** (the engine moved on) is refused
  and shown in words with nothing queued; **exhausted substitutions** (`no_substitutions_left`) shown, queue
  empty; a queued substitution **restored from a save** is shown and its players are not offered again; a
  formation change goes out as the ordinary `Tactics` order; switching to the tab and back keeps the live pitch.
* Panel tests (`FormationSubstitutionsPanel.test.tsx`, 16): 4-4-2, 4-3-3, 3-5-2 each show eleven named
  tokens; a team of ten (dismissal) shows ten and lists the dismissed player; substituted/injured/dismissed
  told apart; the bench is closed until a player is chosen; one order with the two chosen ids and nothing
  moved locally; a queued substitution blocks its two players; cancel; exhausted allowance; orders closed
  while the picture is behind; a selection dropped when the player leaves the pitch; formation change;
  an injured player named in words; keyboard Enter/Space; back to live.
* "Do not allow a substitution twice": a player queued to go off cannot be chosen again as outgoing (the
  hint says to cancel it first); a substitute queued to come on is disabled in the bench (a test selects
  a *different* outgoing player to prove it is that rule and not "nothing selected"; the first version of
  that test let the mutation survive, see B/mutations).
* "Do not allow a dismissed or substituted player to be selected incorrectly": they are in `used` /
  `sent_off`, rendered as list items with a badge, never as buttons (tested).

## F. Simulation report: provenance and financial reconciliation (step 3)

### F.1 Where the report comes from

`FastForwardSummary.report` (`ofm_core::simulation_report`, built by `FastForward::finish`) is returned by the
backend with the run. It keeps no ledger and repeats no simulation:

| Section | Read from | Notes |
|---|---|---|
| Matches | the competitions' completed fixtures involving the club, dated in the days played | fixture and competition ids, opponent id and name, score and shootout from the club's side; W/D/L by the goals of the match (a shootout is shown beside it) |
| Transfers | each player's **movement ledger** (`movement_history`), entries involving the club | purchase, sale, loan in/out, loan returned/ended, free signing; fee, counterparty id and name, loan end; every fee carries `fee_in_journal`: whether the cash journal holds a post of that kind, signed amount, club and date (`false` is shown to the player as a discrepancy) |
| Contracts | the same ledgers | renewals with wage and end **before and after** (the previous contract entry of the same ledger), releases with their recorded reason (expired / terminated / released-reason-not-recorded) |
| Finances | the **cash journal** (`CashJournal`) posts of the club dated in the days played, plus the club's cash before the first day and after the last | by kind (income, expense, net, posts); `unexplained` = change in cash − Σ posts, reported not hidden; `detail_complete` false when a folded post stands in for part of the interval; an `OpeningBalance` seed is neither income nor expense |
| Squad | the club's player ids before and after the run (difference), the ledger's cause attached when it has one (none when it has not), and the injuries the run **watched** | no cause is inferred; `injuries` are first-seen / recovered-by days observed once a day |
| Delegation | `game.delegation.log` entries dated in the days played | counts by outcome (executed / approved / declined / left to the manager) |
| Events | season rollovers the run performed, knockout ties the club won or lost (`advancing_team_id`), board directives in the inbox (id and key), dismissal | |

### F.2 The days actually played, and who was managed

* `from` is the first day played, `until` the first day **not** played. A run that stops early reports the
  days it played, not the days it was asked for (`a_run_that_stops_early_reports_the_days_it_played` in the
  unit tests and in the real-game test, where a decision stopped a 90-day request early).
* A `ReportTracker` inside the run follows `manager.team_id` after each day (and after a rollover): a
  dismissal or a change of club **closes the stretch that day** and the next club gets its own entry covering
  only its own days (unit tests: `a_dismissal_closes_the_stretch_and_later_days_are_not_attributed`,
  `a_change_of_club_gives_each_club_its_own_stretch`). A manager with no club has an empty report and the
  screen says so. (Within one run the club only changes through a dismissal: the loop stops on it; the
  structure supports a move because the tracker compares the club every day.)
* The tracker's whole cost per day is one pass over the club's players; there is no daily copy of the game.

### F.3 Reconciliation, on the real game

`season-harness/tests/simulation_report.rs` plays ninety days of **Total delegation through
`fast_forward::run`** (the real daily turn: AI clubs, windows, registration, finance posting) and checks the
report against counts made another way from the same game: the matches equal the matches the run counted
(`user_matches_played`); purchases, sales and fees equal the **competitions' transfer logs**; opening plus
the journal's posts (taken **by id against the journal as it stood before**) equals closing to the cent;
every by-kind total equals the sum of those posts; the report's transfer-fee totals equal the journal's
`TransferFeeOut` / `TransferFeeIn`; every fee is in the journal (`fee_in_journal == true`); the squad is the
difference of the squad before and after, each sale's cause is `permanent_transfer` with the log's fee; the
delegation counts equal the log's. Observed (printed by the tests, `--nocapture`):

```
seed 1: 90 days, 13 matches (7W 3D 3L, 27-11), transfers 2 bought (8,750,000) 3 sold (5,413,967) 0 loans, 0 contract changes,
        squad 23 -> 22 (2 in, 3 out), 2 injuries, delegation 69 executed 0 declined 0 left, cash 40,000,000 -> 45,264,239 (38 posts, 0 unexplained)
seed 2: 90 days, 13 matches (6W 2D 5L, 30-31), transfers 1 bought (3,450,000) 0 sold, squad 23 -> 24, 5 injuries,
        delegation 57 executed, cash 40,000,000 -> 46,811,071 (34 posts, 0 unexplained)
seed 3: 90 days, 13 matches (4W 2D 7L, 18-25), transfers 1 bought (6,000,000) 1 sold (1,304,661), squad 23 -> 23, 9 injuries,
        delegation 60 executed, cash 40,000,000 -> 44,244,587 (35 posts, 0 unexplained)
fm24 PSG: 90 days, 5 matches (1W 1D 3L, 8-10), transfers 2 bought (97,800,000) 4 sold (2,082,123), squad 49 -> 47 (2 in, 4 out),
        6 injuries, delegation 107 executed, cash 400,000,000 -> 311,657,219 (63 posts, 0 unexplained)
```

Also through `fast_forward::run`: a run across a season end (the rollover is an event, `seasons_rolled_over`
equals the rollover events, the books reconcile across it), a real dismissal (`ManagerFired`, stretch
`Dismissed`, the event, `until` = the stop date), an early stop (days played < 90, reconciles) and a two-day
interval with nothing in it (empty lists, zero totals, reconciles).

The source-backed run is the same chain on the owner's FM24 world; it matches ALPHA6.1's observations
(2 signings for €97.8 M, 4 sales for €2.08 M).

### F.4 Honest limits of the report

* No loan, no contract renewal and no refused offer occurred in the generated 90-day runs; the **loan and
  contract lines are covered by the unit tests** on hand-built ledgers (`transfers_carry_fee_counterparty_and_whether_the_journal_paid_them`,
  `contract_changes_show_the_terms_before_and_after`), and by the source-backed run only where the world
  produced them (it produced none either). A "refused transfer" is not a recorded event the game keeps
  except as a `declined` delegation entry or a decision the manager passed; the report shows those and
  invents nothing else.
* Continental **qualification** is not itemised (nothing records it as an event); elimination and
  advancement of a knockout tie are, as is a promotion/relegation at a rollover.
* Injuries are seen at the end of a day: an injury that began and ended inside one day is not seen, and the
  injury's name is shown as the game stores it (no existing translation of injury names was found to reuse).
* The folded-journal case (`detail_complete == false`) needs a run longer than 730 days; it is unit-tested
  only by construction of the rule, not exercised end to end.
* Drill-down: players and clubs open the existing player and team profile, a board message opens its inbox
  message, a knockout tie opens the Schedule. There is **no per-match or per-competition detail screen** to
  link to; fixture and competition ids are in the data for when there is one.

### F.5 The screen

`FastForwardDialog` gained a **report** phase: from the result ("Full report"), or after the dialog is closed
from the header's "Last report" button (the last run's response is kept by `useFastForward.lastRun`, bound to
the career on screen: another career's report is never offered). Tabs: Summary (a stat block per section and
the important events), Matches, Transfers (with contracts), Finances (by-kind table, reconciliation line or an
alert for what the journal does not explain), Squad, Delegation, Timeline (every dated item, oldest first). Long
lists say "and N more" from the backend's exact total. It states partial coverage ("N of the M days
requested"), shows the run's error, says when there is no club or no report, and separates club stretches with a
second tab bar. **Opening it calls no command**: tested at the hook (three open/leave cycles, the backend is
asked only for the run), at the view (the game store is identical object-for-object after browsing every
tab three times) and through the dialog's own Back/Escape handling.

## G. Full-delegation regression tests

No change was made to the delegation logic, the transfer rules, registration, the valuation scale or the
calendar: none of those files is in the diff. The ALPHA6.1 ninety-day suite
(`season-harness/tests/delegation_90_days.rs`, six tests: list/sell/recruit on seeds 1–3, the controlled sale,
the protected player, the source-backed PSG run) stays in the source-backed run below. The new report is
checked on the **same Total-delegation careers** (seeds 1–3 and PSG), with the same ninety days, through
`fast_forward::run` instead of the harness's `play_day`: the numbers agree with ALPHA6.1's (seed 1: 3 sales
€5.41 M, 2 signings €8.75 M, cash 40.0 → 45.3 M; PSG: 2 signings €97.8 M, 4 sales €2.08 M), which is one more
piece of evidence that fast-forward and the daily turn play the same game
(`fast_forward.rs` compares worlds by fingerprint and was run unchanged: results in J).

## H. Save/reload compatibility

* `Game::state_revision` is `#[serde(default)]`, **not persisted**: `db::game_persistence` builds the loaded game
  with `state_revision: 0` and `career_token: 0`, `install_career` then stamps a fresh token and `set_game`
  gives the loaded career its first revision. The save's SQLite schema and the transaction ledger are unchanged;
  no migration. The mcp-e2e save/reload comparison lists `game.state_revision` next to `game.career_token` as
  a deliberately unsaved session field.
* Old saves: nothing in them mentions the field; every `Game` read from an old save loads as before
  (the db round-trip suites and the source-backed `i1_fm24_reload` / 7-old-saves journey stay in the workspace
  and harness runs below).
* `InteractiveView.formation_shapes` is a wire-only field (the view is `Serialize` only); a match saved in
  progress restores through `saved.rs` unchanged and the restored view computes its shapes from the engine.
* The report is not saved: it describes a run that has just happened. Reloading a save in the same window ends
  the old career's `lastRun` (its career token differs).

## I. Performance

* **Revision:** one `u64` increment per write under the lock, and one more field on every `Game` sent. The
  transaction still takes one `ui_clone` (4.8 ms on the 16,609-player world, ALPHA6); no snapshot is stored.
  Not benchmarked separately: the cost is below what the tests can resolve.
* **Report, measured on the FM24 world (16,609 players, release build), printed by the source-backed test:**
  the read at the end of a run **286 µs**; the watch per day **23 µs**. (Measured on the finished game with a
  fresh tracker, so the end-of-run read scans every fixture, ledger and journal post once; the opening
  `PreRun` capture, one more pass of the same kind, was not timed separately.) Against a 90-day run that takes tens
  of seconds, the report is not a cost. No daily clone of the game exists in the report path.
* **Frontend:** the report holds at most 400 items per list; the panel's `keepApart` is a bounded loop (24 passes
  over eleven tokens); the 72 browser runs showed no layout cost worth recording.
* **M12:** `formation_shapes` (6 formations × 11 slots) is serialised with every `InteractiveView`, like
  `supported_formations`; not measured, and worth sending once if the view is ever polled per frame.

## J. Automated test results

All on the final code, in this session (`apfm/main` working tree before the commits).

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean (both) |
| Rust workspace tests, default | **5,733 passed, 0 failed**, 160 ignored |
| Rust workspace tests, `--features mcp` | **5,820 passed, 0 failed**, 160 ignored |
| Source-backed: `cargo test --release -p fm24-adapter -p season-harness --no-fail-fast`, `APFM_WORLD_JSON=~/apfm-scratch/alpha6-world/apfm-fm24-2026.json`, `APFM_WORLD_DB=~/apfm-scratch/world-copy.db` (a **copy**) | **85 test binaries, 367 passed, 0 failed**, 100 ignored, **no `SKIPPED` output** (ALPHA6.1: 84 binaries, 359 passed; the 8 additional passes are this mission's 8 report tests) |
| New Rust tests | `ofm_core` revision 9 (5 in `state.rs`, 4 in `transactions.rs`), `simulation_report` 19, `interactive_match` formation shapes 2; `season-harness/tests/simulation_report.rs` 8 |
| Vitest (whole suite) | **3,580 passed**, 4 skipped (pre-existing), 400 files |
| `tsc` + `vite build` | clean |
| Biome format / lint | clean (3 pre-existing infos) |
| Knip | clean (2 configuration hints, pre-existing) |
| i18n suites (12 locales), architecture and design-system audits | green (37 i18n tests) |
| Real-browser measurement of the real M12 screen | 72 runs, 0 issues (section D) |
| Mutation checks | revision 9, M12 17, report 19, all killed (K.6) |
| Reviewers | 5 run; findings in K |

Three test runs earlier in the session were red for reasons recorded here and fixed: six ALPHA6 tests asserting
"a refusal leaves the game byte for byte" went red when the revision first advanced on a refusal (fixed by
`update_game_when`, two of them also compared a game captured before `set_game` stamped it); a first full run
of the source-backed report test stopped on a decision rather than at a match (a wrong assumption in the
test); the layout test's stricter touchline check found a real 5-3-2 defect.

**Not run or not claimed:** a ≥730-day simulation, the macOS Tauri GUI, a light-theme/high-contrast/compact
visual check, `ui_scale` ≠ 1, a real Tab-key walk, four Rust mutants (K.6).

## K. Reviewer findings

Five read-only reviewers were run on the diff (the repository's `i18n-auditor`, `ui-accessibility-reviewer`,
`ofm-dedup-reviewer`, `ofm-architecture-reviewer`, `ofm-test-reviewer`). None reported a P0. What they found and
what was done:

### K.1 i18n-auditor — fixed

| Finding | Action |
|---|---|
| **Delegation outcome "left to you" rendered as a raw key in all 12 locales**: the backend serialises `leftToManager` (camelCase), the locale key was `left_to_manager` | key renamed in all 12 locales; fixture and test now include a `leftToManager` entry and assert no raw key |
| Injury names shown as raw keys (`common.injuries.twistedAnkle`) | `resolveInjuryName` (the game's own helper) now used; fixture uses a real key; tested |
| Templated competition names rendered as `{{country}} First Division` (no nation/season passed); knockout lines printed the English name | the report resolves the competition by id from the game (`competitionDisplayName`, the helper the end-of-season screen uses), falling back to the report's own name; tested with a templated name |
| Engine position codes (`CB`, `GK`) shown raw | `translatePositionAbbreviation` on every position the panel prints (token, bench, detail, screen-reader description) |
| "for {{fee}}" with a word ("free") in the price slot is ungrammatical in 9 languages; "from {{club}}" with a baked-in masculine article (`dal`, `del`) is wrong for most club names; "another club" in a case-governed slot | the nine transfer sentences were rewritten without the fee and without articles, the fee is its own clause (`Fee: {{fee}}`, a free signing has none), the unknown club is a parenthesised neutral phrase |
| Gendered participles addressing the manager (es/pt/fr/it "dismissed") | rephrased impersonally (`El club te despidió`, `Il club ti ha esonerato`, …) |
| Wrong sense or form: pt-BR "Jogadas", fr "retraité", cs "Čistě", fr "1e", ru "21 процентов", es "Abrir a {{name}}", tr informal "al", zh stilted, fr "Laissé à vous", ru mixed division wording | corrected |
| Reported, not changed (advisory) | `{{windows}} windows left` and `{{before}} players` have no plural forms in any language (only en/ru would visibly break at 1); the cs `leftToYou` register differs from an older cs key |

The reviewer verified the 14 `INTENTIONAL_SAME` additions are genuine same-in-language words.

### K.2 ui-accessibility-reviewer — fixed unless stated

| Finding | Action |
|---|---|
| P1 `text-on-brand` on `-solid` fills carried the only non-colour cue for injured/queued markers (≈2.1:1 and 3.8:1) | markers are now `bg-panel` discs with a `border-danger`/`border-warning` ring and a coloured glyph |
| P1 token text sat directly on the emerald gradient (≈2.5:1 for white 12 px text) | name and role/condition each sit on a `bg-chrome/75` pill; **not re-measured for contrast by a tool** |
| P2 the selected ring used the focus colour, which equals the turf at the bottom of the pitch in light theme | the ring is `ring-on-brand`, plus the white fill, underline and `aria-pressed` |
| P2 the chosen bench player was shown by a 1 px colour change | bold, a ring and a ✓ |
| P2 the status hint was mounted with its text (not announced) | one always-mounted `role="status"` whose text changes |
| P2 dangling `aria-controls` on the club tabs | each club has its `TabPanel` (none is rendered for a single club); tested that every tab's `aria-controls` resolves |
| P2 focus lost when the dialog changes phase | the report's title takes focus on mount; tested |
| P2 `md:` breakpoint with a rem-fixed pitch breaks at `ui_scale` > 1 | a container query (`@min-[52rem]`) on the card body; **not measured at scale ≠ 1** |
| P2 hover colour lowered link contrast; "Clear selection" disabled itself while focused | `hover:bg-hover`; focus moves to the panel title first |
| Not changed | the pre-existing keyboard focus ring colour on `FormationPitch` tokens (accent-300/70) — judged justified but undocumented; no roving tabindex on the pitch (11 tab stops); `CardHeader` always renders an `h3` so the panel's sections are siblings of its title; `FormationPitch`'s hover scale has no `motion-reduce` guard (pre-existing) |

### K.3 ofm-dedup-reviewer — fixed unless stated

| Finding | Action |
|---|---|
| `competitionDisplayName` rebuilt wrongly | see K.1 |
| `kind_name` re-spelled `PlayerMovementKind` | the report carries the enum (`SquadMove.cause: Option<PlayerMovementKind>`; TS `PlayerMovementKind | null`) |
| `squad_ids`/injury scan repeat `club_players` | `match_day_eligibility::club_players` reused |
| the fixtures file was a third hand-copied formation table | `SHAPES` is now the engine's own table from the real generated fixture (`liveMatch.real.generated.json`) |
| local `POSITION_GROUPS` duplicates `normalisePosition` | replaced by it (`ss` added to the shared map) |
| `division_move` from a lowercased Debug string | an exhaustive `match` on `UserDivisionMove` |
| Named for epic #589, not consolidated | the older `SubPanel.tsx` (half-time) beside the new `FormationSubstitutionsPanel` (different data models); `group_stage.rs`'s private `is_knockout_fixture`; `fastForward.report.event.divisionMove.*` and `.cause.*` next to `endOfSeason.*` / `playerProfile.movement*` labels (wording differs; shared label table would be better); `toPitchPlayer` zero-fills an `EnginePlayerData` because `FormationPitch` wants the wide type; `squad.rs::name_of` is a linear scan per listed player (capped at 400) |

### K.4 ofm-architecture-reviewer — fixed unless stated

| Finding | Action |
|---|---|
| **P1** the report counted things recorded **before the run on its first day** (the clock's day is shared: a payment, finished match, signing, delegation step or board message dated that day but made before the button was pressed would be counted, and `unexplained` would show the payment) | a `PreRun` baseline captured when each stretch opens (post ids, completed fixture ids, per-player same-day ledger counts, delegation count, message ids); the readers set those aside; unit test `what_was_recorded_before_the_run_on_its_first_day_is_not_the_runs` (red before: the earlier payment and the earlier match were counted); the first-day fixture test was changed so its match is *played during* the run |
| P2 `install_career_for_screen` read the revision back after the install (a writer in between would make the screen's copy look newer than its content) | `set_game` returns the revision it assigned; the screen's copy is stamped with that |
| P2 `mutate_active_game` refusals advanced the revision | it uses `update_game_when` (refusal leaves the game untouched) |
| P2 `InteractiveMatchScreen` (216 → 259 lines) and `Dashboard` grew | the live-match body is `LiveMatchView.tsx` and the screen is back under its HEAD size; the report's navigation is `reportNavigation()` (`Dashboard` is still far over the limit it was already over; not split further here) |
| Observations | `MAX_LISTED` and `ReportEvent::date` are no longer `pub`; the same-day ledger order of a player's moves is now ledger order; `write_formation_probe` panics without its output variable; `formation_shapes` is re-serialised with every view (like `supported_formations`: send once if the view is ever polled per frame — not changed); `openReport` has no in-flight guard (the modal covers the button during a run) |

### K.5 ofm-test-reviewer — fixed unless stated

Observed by the reviewer in a throwaway copy: 11 frontend mutations killed, 7 survived. What was done:

| Surviving mutation / gap | Action |
|---|---|
| `placeOnPitch` matching by list order instead of role survived (the fixture lists players in slot order, and the ten-man test removed the last index) | two tests: a midfielder dismissed from the **middle**, and two players swapped in the list; the mutant is now killed |
| the chosen substitute surviving a change of outgoing player; the focus move before a button disables; a substitute queued after being chosen | three panel tests; mutants killed |
| side margin / clamp removable | the layout test now checks the **whole footprint inside the drawn markings** (this found a real defect in 5-3-2, fixed); mutant killed |
| a partial stamp (career but no revision) | a store test |
| the report: dynamic keys asserted through the same `tr` call | `reportKeys.test.ts` enumerates every backend enum value (a `Record` keyed by the TS union, so the compiler rejects a missing value) against `en.json`; the board-message and rollover test lists exact events with an out-of-range and a non-board message; the browsed-report test seeds a real game; the store is reset in `afterEach` |
| Rust report gaps | tests for a folded post inside the run, a loan returned/ended, a purchase through an option, a termination and a release with no reason; a dead loop and a tautological assertion removed; the injury doc now says "seen while it played" |
| **no end-to-end test of the report across a season end or a dismissal** (the tracker hooks in `fast_forward.rs` were called by hand only) | two harness tests through `fast_forward::run`: a run across a season end (the rollover is an event, the books reconcile across it) and a real dismissal (`ManagerFired`, stretch `Dismissed`, the event); both pass |
| the source-backed test passes when it plays nothing (prints `SKIPPED`) | **not changed**: it follows the repository's convention (`delegation_90_days.rs`) so the default suite is not red without a world; it **was run with `APFM_WORLD_JSON` and `APFM_WORLD_DB` set** (section J) |
| replay test name, concurrency test wording, a screen-side `>=` loop re-implemented in a Rust test | renamed/removed; the Rust test asserts the stamps only |
| Reported, not changed | the screen-level M12 fake never applies a queued substitution at a stoppage and has two players per side (the panel is tested with full 11-man teams at panel level, the screen with the fake); `the_roles_on_the_pitch_fill_the_slots_of_the_formation_shape` checks only the formations a started match uses (the six-formation probe is an ignored generator); nothing ties `liveMatch.real.generated.json` to the Rust output except regenerating it; `install_career_for_screen`'s stamping has no direct test; a refused-transfer-through-`submitTransferCommand` reversal feeds the store by hand; `fee_in_journal` and sale-cause loops in the harness would pass if their iteration were empty (the printed counts show they are not) |

### K.6 Mutation checks (rule removed, named tests red, file restored byte for byte)

Revision: frontend 3 killed (revisions compared across careers; a replaced career accepted; the store skips the gate);
Rust 6 killed (no bump before the closure — and the stamp one behind, which needed one more assertion; `set_game`
not advancing; a refusal keeping the bump; a closure that swaps the game rewinding it). **M12 panel:** frontend
13 killed (shapes ignored; tokens not kept apart; the queue not subtracted from the allowance; exhausted
allowance still choosable; confirm swapping the two ids; injured shown as substituted; a queued substitute
choosable again — **survived once** until the test selected a different outgoing player; cancel sending the wrong
player; formation change sending the current formation; the screen not routing orders) plus 4 re-runs after the
reviewer's survivors (list-order matching; selection keeping the substitute; confirm not moving focus; the side
margin). **Report:** frontend 9 killed (partial coverage never stated; timeline unsorted — the first version was
a syntax error and was redone; omitted count hidden; last report forgotten; another career's report offered; unpaid
fee not flagged; unexplained cash not flagged; names never link); Rust 10 killed (the first unplayed day counted
for matches and for cash; every fee "in the journal"; days not counted; a change of club not followed;
delegation outside the run; departures = arrivals; a recovery never recorded; lists not capped). **Not run** (each
needs a recompile of `ofm_core`, ~7 minutes): new-career-restart, opening-balance-as-income, previous-contract
terms, purchase/sale swapped; the first three have a test that would be expected to catch them.

## L. Remaining defects, unverified items and recommendation

**Confirmed and fixed in this mission:** out-of-order full-game answers (B.1); a report that counted
first-day records made before the run (K.4); the delegation outcome shown as a raw key, raw injury and
competition names, ungrammatical fee wording (K.1); touchline and name-truncation defects in the panel
(D).

**Open, in order of consequence**

1. **The Tauri GUI was not operated** (N). Everything above is proved in Rust, in jsdom and in headless
   Chrome on the real components with mocked IPC; none of it is a session of the real app.
2. **Out-of-order answers are fixed for full-game answers only.** Commands that return a slice of the game
   (the inbox) are not stamped (B.3); a late answer of the career just left, after `clearGame`, is not
   recognised by revision.
3. **Accessibility measured only partly:** contrast of the new name/condition pills and markers is by
   token reasoning, not measured; light theme, high contrast, compact density, UI scale ≠ 1 and a real Tab
   walk were not exercised. The pitch's own keyboard-focus ring colour (pre-existing) and the absence of a
   roving tabindex on the pitch were left.
4. **Report coverage in the generated and FM24 90-day runs:** no loan, renewal, refused transfer or
   injured-and-recovered case occurred, so those lines are covered by unit tests on hand-built ledgers, not by a
   real run; the folded-journal notice needs a run over 730 days and is unit-tested only; continental
   qualification is not itemised; there is no per-match or per-competition detail screen to drill down to.
5. **M12 screen-level coverage is thinner than panel-level:** the shared engine fake never applies a queued
   substitution at a stoppage (the Rust controller's own tests do), and a red card / injury-off came from
   real engine data with the dismissal applied to the view, not from an engine that dismissed a player.
6. **Not run:** four Rust mutants (K.6), the macOS GUI, a ≥730-day run, a light-theme visual check.
7. `Dashboard` and `useLiveMatch`-sized functions that were already over the 200-line limit remain over it;
   duplicates named for epic #589 are listed in K.3.
8. Old open items carried unchanged from ALPHA6.1: AI-market volume policy (hold on loaned-out places) and the
   calendar tail (a planner is needed); neither was authorised or touched here.

**Recommendation:** CONDITIONAL GO for owner testing of ALPHA6.2 and for the independent audit, **NO-GO for
any claim that the interface works end to end** until someone operates the app (section N).

## M. Source database integrity

`~/apfootballmanager/data/world.db` SHA-256 `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` (re-read at the end: identical); it was `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` at the start. Every source-backed run read `~/apfm-scratch/world-copy.db` (a copy, SHA-256 `36b6908c…` at the time of the runs) and the FM24 world JSON from the previous mission; nothing was installed over a save or the real database, and no owner save was opened.

## N. Minimum manual acceptance checks that cannot be automated

Nobody operated the macOS Tauri app in this mission. The minimum a person must do:

1. **Order flow in the real app.** Start a match in "Manage Match in 2D". Open **Formation & Substitutions**,
   pause, select a player, select a substitute, confirm; resume until the next stoppage and check the
   substitution happens and the pitch/list agree. Cancel one before the stoppage. Try a sixth substitution.
2. **Keyboard and screen reader.** Tab through the tokens (Enter/Space selects), the bench, Confirm, Cancel, the
   formation select and "Back to the live match"; with VoiceOver check that a token reads name, position,
   condition and "waiting for a stoppage"/"injured", and that a refusal is announced.
3. **Themes.** Light theme, high contrast, compact density and a UI scale of 125–150 % on the panel (the pitch,
   the markers, the text over the turf).
4. **A real dismissal and a red card** (the ten-man pitch was measured from real engine data with the red card
   applied to the view, not from an engine that actually dismissed a player).
5. **Simulation report.** "Simulate 3 months" with full delegation, then read the report: do the transfer lines
   match the Transfers page and the cash line the Finances page? Close the dialog, reopen "Last report"; follow
   a player, a club, a board message and a knockout line; check one early stop (Stop button) and a run that is
   interrupted by a decision.
6. **Out-of-order answers.** Hard to force by hand: start two transfers (a bid and a loan) in quick succession
   with the Network panel throttled, and confirm the screen ends on the later state; after an MCP-driven
   change (`game-state-changed`) confirm the dashboard shows it.
7. **A 730+ day simulation** (the folded-journal notice) and a world in which loans and renewals occur.
8. **Old saves** from before this change: load, simulate a week, save, reload.

---
Implementation, tests and this report were pushed at `b83aecd780e52355672fd8332f3566cb5c17eff9` (fast-forward from `a841051c`); this line is a documentation-only commit on top of it.
