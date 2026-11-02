# APFM — Run F1: stabilising the blocking bugs seen in play

Branch `integration/gameplay-runs-a-d` (head `6b5126a`, Runs A–E merged). A test career with Schalke 04
showed five anomalies while the automated suites were green. Each section gives the root cause that was
**established** (by running code, not by guessing), what changed, the tests, and what is still not known.

What could not be run in this session is in [§7](#7-what-was-not-validated-and-what-to-run-locally): the real
M12 engine, the user's FM24 `world.db` and the user's save were not available, so a claim about them is a claim
about code, and says so.

| # | Anomaly | Root cause (short) | Commit |
|---|---|---|---|
| 1 | Free agents rated 79–82, generic names, millions in value | Two sources: national-squad fillers left on the market, and opening free agents drawn better than the clubs' players | `fix(market)` |
| 2 | "Squad full" with no number after releasing three players | A fixed 30-place senior list, refused without saying how full it was or what counts | `feat(squad)` |
| 3 | Paid transfer completed with no visible step with the player | A keen player on the buyer's standard contract was judged inside the bid call and completed on the spot | `feat(transfers)` |
| 4 | "Watch in 2D" → "This replay is unavailable or corrupted" | Shirt-number assignment in the M12 adapter failed the whole match; the screen hid every error behind one message | `fix(match)` |
| 5 | The contract alert interrupts the advance every day | The blocker was recomputed daily with no memory of being dealt with, and "Continue anyway" could not continue | `fix(contracts)` |

---

## 1. Free-agent market

### What was measured

`ofm_core/tests/market_initial_consistency.rs` (seeds 20260820, 1, 7, 9001). On seed 20260820, before any change:

* the 1,760 opening free agents were **better than the players clubs hold**: median rating 66 against 63, 47 rated 79 or more
  (best 84);
* scheduling a World Cup added 34 made-up players to `game.players`, rated up to 78 and valued up to 4.2 M€.

### Root causes

1. **National-squad fillers.** `world_cup::prepare_national_squads` tops every nation in the field up to 18 players
   with `generator::generate_national_team_player` so a national squad can be named. Those players belong to no club
   and had nothing to say they were not free agents, so the free-agent list, the offers, the rival-interest sweep and the
   AI squad floor all treated them as such. They are drawn uniformly, then the best 23 of each nation are selected, so
   the ones that surface in a "best free agents" view are the top of a big random draw: high rated, a name from a pool
   (nations without their own pool borrow another's, hence names that do not match the nationality) and a value from the
   same formula as every other player (`ovr² × 500 × age factor`, millions at 80). This is what "Adam Parker, Jack
   Johnson, Igor Ribeiro" are.
2. **Opening free agents.** `generate_opening_free_agents` used the same unconditional distribution as the club squads.

### Fix

* `domain::player`: `NATIONAL_POOL_ID_PREFIX = "np-"`, `Player::is_national_pool_filler()`, `Player::is_on_free_market()`.
  New fillers get the prefix. The id is a plain string every save already stores: **no column, no migration**.
* A save written before the prefix is recognised by what only such a player looks like: unattached, still wearing the
  generator's shirt number, and **no recorded movement**. A released or sold player has a movement; the FM24 import and
  packages give nobody a shirt (`pipeline.rs:925`; `PlayerDef` generation sets none). **No player is deleted or
  renamed**: they stay in `game.players` for the national team.
* `is_on_free_market()` is used by the free-agent list and the `Available` filter (`slices/players.rs`), the talk
  entry point (`free_player_index`), the rival-interest sweep and the AI squad-floor signing (`squad_floor.rs`).
* Opening free agents are drawn under the clubs' 75th percentile (`OPENING_FREE_AGENT_CEILING_PERCENTILE`; up to 12 draws,
  the weakest kept if none fits).

A given seed now generates a different (still deterministic) world, as with the Run D name fix. Saves are untouched.

### Tests (`market_initial_consistency.rs`, `domain::player::tests`)

* `the_opening_free_agents_are_not_better_than_the_players_clubs_hold` (4 seeds: best ≤ clubs' P75, median ≤ clubs' median, none at the clubs' P90).
* `national_squad_fillers_never_reach_the_free_agent_market` (World Cup scheduled; fillers exist; the market is exactly what it was).
* `an_older_save_keeps_its_released_players_and_hides_its_fillers`, plus three unit tests of the two predicates.

### Limits

* **The user's FM24 world was not available.** The FM24 pool (`fm24-adapter/free_agents.rs`) is real FM24 people chosen by
  a seeded hash and priced from the save's own wages; it was not changed. If a 79–82 free agent with a *real* FM24 name
  shows up, it is FM24 data, not this bug. The generic names in the report match fillers, not the importer.
* The legacy rule can hide an unattached player with a shirt and no history that is *not* a filler. None of the code paths
  that make players creates one; a hand-edited save could.
* The made-up players are not rated by nation strength: a weak nation can still get a strong filler. Out of scope here.

---

## 2. Squad size blocking signings

### What was established

`contracts/free_agent_talks`: a club is refused a free agent when `ClubSquad::seniors() >= MAX_SENIOR_SQUAD` (30). That
count is every player at the club with the `Senior` role. The refusal key (`be.error.freeAgents.squadFull`) carried
**no number**. 33 → 30 after three releases is still full (30 of 30); the manager read it as "the release did not count".
The FM24 importer gives the `Senior` role to every player over the academy age, including reserve-team players
(`fm24-adapter/placement.rs::role_at`), so a real club can sit above 30 from day one.

There was no stale-state bug in the count: releasing a player clears `team_id` in the same step
(`contracts::release_player_contract`) and `terminate_contract_now` returns the updated game.

### Fix

* `contracts::squad_capacity(game, club)`, the **single** authority: seniors at the club (loanees from other clubs
  included), youth (no place on the list), loaned in, loaned out (registered by the borrower, not counted), free places,
  places to free, `blocks_signing`. Retired players hold none.
* The refusal is `be.error.freeAgents.squadFull?count=30&max=30&release=1` and the text (12 locales) says what to do.
* `preview_free_agent_contract_impact` returns `squad_capacity`; the contract screen shows the summary line, the places left
  or the "free N before signing" alert, **and says it is the club's registration limit, not the player's refusal**. The submit
  button is disabled while the list blocks.
* **Administrative limit vs impossibility**: the list is the only thing this code calls administrative, and it is resolvable
  by the manager (release, sell, loan out, move to youth). Money (board wage policy, bonus, cash) and the player's answer keep
  their own keys. The cap itself was *not* raised or removed.

### Tests

`capacity::tests` (limit boundaries; 33 → 30 releases with exact numbers; youth/retired/lent-out/borrowed),
`free_agent_talks_tests::a_full_senior_list_says_how_full_it_is_and_opens_after_exactly_enough_releases`,
`FreeAgentTalkPanel.test.tsx` (full and with-room renderings).

### Limits

* The 30-place limit is unchanged and still stops AI rivals too. For an FM24 club with a large reserve squad, the way to
  free places is to release or sell the surplus; there is no "reserve" role to move them to (`SquadRole` is `Senior | Youth`
  and adding one is a save-format change that was not made here).
* The limit applies to free-agent signings only (as before). Paid transfers, loans and youth promotions do not check it.

---

## 3. Paid transfers: the player's contract

### The path audited

React `TransferBidModal` / `useTransferBidFlow` → `services/transfersService.makeTransferBid` → Tauri
`make_transfer_bid` → `make_transfer_bid_internal` (shared with the MCP `transfer_make_bid`) → `ofm_core::transfers::bids` →
`consent.rs::judge_in_game` → `execute_transfer_with_terms` → `terms::{settle_cash, record_obligations}` →
player `transfer_offers` (JSON) + `transfer_obligations` table.

Run E added the player's judgement (`consent.rs`) and `TransferTerms.player_weekly_wage / player_contract_years`. It was
**enforced** but not **visible**: a bid whose clubs agreed, on a player who would take the buyer's standard contract
(`player_weekly_wage = 0`), completed inside the same call. The player never "spoke". Only a hesitant or unwilling player
produced an answer the manager could see.

### Fix

* `make_manager_transfer_bid` (used by the Tauri command, hence the MCP tool). When the clubs agree and the bid names no
  contract for the player, **nothing is completed**: the answer is the new `TransferNegotiationDecision::PlayerTalks`
  with `player_demand` = the contract he would sign (wage, years), a positive feedback
  (`transferFeedbackPlayerTerms*`) and no stored offer, no cash movement, no history entry.
* The manager then puts a contract to him (the form is pre-filled with his terms and can be edited). He answers with the
  existing outcomes: **accepts** (the transfer completes, at that contract), **counter-proposes** (`Rejected`, non-final, with
  his ask — the form is pre-filled with it as well), or **refuses** (no counter). The fee negotiation between clubs is unchanged
  and runs first.
* The assistant coach (`make_transfer_bid_with_terms`) keeps judging the standard contract on the spot: it acts inside limits
  the manager set and has no screen. This is the only path where the contract is *assumed*.
* Frontend: `useTransferBidFlow` puts the player's terms into the form on `player_talks` / his counter; the modal shows
  "clubs agreed — now agree his contract" and the button reads "Put contract to player".
* AI sweeps, the user's answer to incoming offers and registration of a deal that waited for the window already re-run the
  player's judgement (Run E, `paid_transfer_consent_tests`); not changed.

### Tests

`manager_transfer_player_step_tests.rs` (7): clubs agreeing completes nothing; accepting the contract shown completes it once
at that wage; a low contract gets a counter, taking the counter completes; an insulting one is refused with no counter; a
player who will not move is refused before any contract; the assistant's path is unchanged (same fixture, completes at once —
which is exactly what happened to the manager before); the fee negotiation is untouched.
Frontend: `useTransferBidFlow.test.tsx`, `TransferBidModal.test.tsx`.

### Limits

* The player still answers the same day (Run E kept that on purpose); the *step* is explicit, not delayed.
* Jordan Henderson's actual save was not available; the behaviour was reproduced on a fixture with the same shape.
* A deal left at the `PlayerTalks` step stores nothing, so closing the modal drops it; re-bidding re-asks the clubs on that
  day's figures.

---

## 4. 2D replay unavailable

### What the code shows (no M12 needed)

Two independent defects, both reproducible without the engine:

1. **`spatial_adapter::shirt_numbers`** handed out default shirt numbers in squad order *before* looking at the numbers the game
   had assigned, and then failed the whole match with `DuplicateShirtNumber`. An FM24-imported squad has no shirts
   (`fm24-adapter/pipeline.rs:925`); `roster::resolve_jersey_for` gives a signing the lowest free number — **1** in a squad
   with none. The first unnumbered starter was given 1 before the signing was reached. Two tests reproduce it
   (`a_player_with_a_shirt_among_unnumbered_players_does_not_abort_the_match`, `two_players_with_the_same_shirt_are_renumbered_not_fatal`),
   both failed with `DuplicateShirtNumber` before the fix. Explicit numbers are now reserved first; a number held twice is
   renumbered and **reported in the adaptation `defaults`**. No M12 input is invented.
2. **The error was hidden.** `prepare_spatial_replay` returned the `Debug` text of the Rust error and
   `MatchSimulation.handleSpatialReplay` replaced every failure with `match.replayUnavailable` ("unavailable or corrupted"),
   so a refused lineup, an engine failure and a corrupt document all looked alike. Errors are now
   `SpatialRunError::translation_key()` → `be.error.spatialReplay.{inputRejected,engineFailed,outputInvalid}` with a technical
   code, resolved on screen. A failure with no key shows a neutral "could not be prepared; nothing was changed"
   (`match.replayPreparationFailed`); "corrupted" is kept for a stored replay the viewer cannot read.

### What is **not** proven

I cannot say which of the two the Schalke match hit: the save and M12 were not available. What the evidence supports: the first is
a certain way to fail "Watch in 2D" for a club that signed a player into an unnumbered squad; the second guarantees the user can now
read the real cause the next time. **No synthetic or substitute replay is ever produced**; `build_adapted_replay` still calls M12 and
nothing else.

### Tests

`spatial_adapter` (2), `spatial_runner::a_failed_preparation_is_reported_with_its_own_key_and_code`, `MatchSimulation.test.tsx` (2),
`backendI18n.test.ts` (2).

---

## 5. Repeated contract alerts

### Root cause

The inbox messages were fine: one per (player, contract end, stage), deduplicated by `emitted_events`. The daily stop came from the
**blockers** (`application/time_blockers.rs`): `key_contract_risk` and `contract_wage_risk` were recomputed every day for any
top-three player within **365 days** of his contract's end, with no memory that the manager had answered the message, opened renewal
talks or chosen to carry on. And "Continue anyway" in the day-by-day advance simply called `advance_one_day` again, which re-computed
the same blockers and returned `blocked` — it could not continue.

### Fix

* `ContractRenewalState.warning_handled` (`"{contract end}|{stage}"`, `#[serde(default)]`; the state is a JSON column — no migration).
* `contracts::contract_needs_attention(player, date)`: only inside the last six months (the first message is at six months),
  not let-expire, not already handled at this stage. The next stage, or a renewed contract, warns again.
* Handled when: the manager answers the player's concern message, opens renewal talks (`propose_renewal`, whatever comes of them),
  sets let-expire (already suppressed), or chooses "Continue anyway".
* `advance_one_day(ignoreBlockers)`: plays that one day past the blockers shown and records the contract warnings among them. It is
  **not** a standing permission: a real squad problem (injured starters, no keeper) is raised again the next day. A new command
  `acknowledge_contract_warnings` is called before the batch flows (Skip, Continue) resume.
* Frontend: the digest's "Continue anyway" passes `ignoreBlockers` to the first day only.

### Tests

`contract_alerts_tests.rs` (5), `commands::time::tests` (3: continuing anyway does not re-raise the same warning; continuing anyway
does not mute a squad problem; the next stage stops the advance again), `useDigestAdvance.test.ts`, `useAdvanceTime.test.tsx` (2).

### Limits

* The assistant's delegated renewals (Run A) are not looked at: a delegated renewal does not mark the warning handled.
* `urgent_messages` (an unread Urgent message) still blocks until read, as designed.

---

## 6. Compatibility

* Saves: no schema change. New serialized data is `ContractRenewalState.warning_handled` (default `None`) and the `np-` id prefix
  (a string). Offers stored before keep their terms. `TransferNegotiationDecision::PlayerTalks` is never persisted.
* Runs A–E, delegation, fast-forward, the FM24 data and the viewer: the only changes in their files are the exhaustive match on the new
  decision in `delegation_recruitment.rs` and the call-site rename in `commands/transfers.rs`.
* `engine` still imports no `domain`; no M12, viewer, `Cargo.toml` or `Cargo.lock` change.

## 7. What was not validated, and what to run locally

* **M12 is absent.** `ofm_core` depends on `../../../../../apfm-match-lab`. To compile, a stub was placed **outside** the repository
  (`/home/apfm-match-lab`, never committed) whose simulation always errors. Everything that executes the engine failed *by
  construction* and proves nothing either way: `spatial_runner::tests::m12_execution_projects_a_deterministic_coherent_match_report` and
  the five `commands::spatial_match` finalisation tests (skipped by name in the runs below). No test reported here ran real M12.
  Run on the Mac: `scripts/apfm/run-m12-tests.sh`, then play one match with "Watch in 2D" on a club that has signed a player.
* The user's FM24 `world.db` and saves were not touched or read.
* The Tauri window was not run; the UI is covered by vitest, the type check and the build.
