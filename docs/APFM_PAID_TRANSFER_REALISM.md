# APFM — Realistic paid transfers and delegated recruitment (Run E)

Branch `claude/gameplay-market-finalization`, based on `integration/gameplay-runs-a-d`
(`05eec51`). What the code did before is in `APFM_PAID_TRANSFER_AUDIT.md`; nothing there that worked
was rebuilt. This document describes what was added, how it is calibrated and what it does not do.

## 1. A paid transfer has three agreements

| Condition | Who decides | Where |
|---|---|---|
| A. The clubs | The selling club's valuation (`minimum_acceptable_fee_at`), counter-offers, rounds, break-off and cool-down | `transfers/bids.rs` — unchanged |
| B. The player | The appeal score below | `transfers/consent.rs` — new |
| C. The books | `fund_block` (cash, transfer budget), the board's wage policy, the squad floor, loan conflicts, duplicate registration | `transfers/terms.rs`, `execution.rs`, `squad_floor` — unchanged, now also asked about the wage actually agreed |

B is checked **after** A and **before** anything is marked agreed, so a refusal leaves no half-agreed
offer. All three are enforced in Rust; the React form only presents them.

## 2. The player's willingness (B)

It is the Run C free-agent appeal (`contracts/free_agent_talks/appeal.rs`): one score, a sum of small
bounded terms, `>= 0` accept, `>= -18` counter, below that a refusal. Nothing is random or stored.
For a free agent nothing changed (their tests pass untouched). A player who already has a club carries an
`Incumbent` block with three differences:

| Term | Free agent | Player under contract |
|---|---|---|
| Sporting | His level against the club's standing (`60 % reputation + 40 % squad level`) | **The club he leaves** against the one he joins: a step up is a draw (up to +12), a drop costs nothing up to 60 standing points, then `-(drop - 60) x ambition / 800`. Ambition is 40-150 % (young, gifted: more; veteran: less) |
| Role | Absolute place in the new side (key +12 … backup -12) | **Difference** between the place he would have and the one he has (clamped -24 … +18): a key man does not become a backup for nothing, a backup is glad of a place |
| Situation | Desperation after 3 weeks out of work (up to +20) | **Push** from what is wrong where he is, capped at +30: listed by his club +8, morale ≤ 45 +8 (≤ 60 +4), contract ending ≤ 180 days +6 (≤ 365 +3), club in debt +5 |

Wage (±: a quarter more pay is 12 points, bounded -25…+20), contract length (he wants his
`expected_contract_years`; a shorter deal costs 6 points a year) and the stepping-stone promise (+8) are
shared with the free-agent model. The signing bonus counts as pay spread over the contract, so the old
"promise or bonus wins a young ambitious player round" survives as two of the terms rather than a special case.

**Not arbitrary.** There is no flat prohibition: every refusal is a score that pay, length, role, a promise or
circumstances can lift, until the term bounds run out. What cannot be bought is what should not be: a
giant's elite player for a minnow on standard terms (-52 sporting against at most +20 for pay and +30 for
circumstances).

### Where it applies

| Path | Behaviour |
|---|---|
| The manager's bid (`make_transfer_bid_with_terms`) | After the clubs agree. Keen: the deal proceeds. Hesitant: a non-terminal `Rejected` with `player_demand` (the wage and length that bring him round) and `transfers.transferFeedbackPlayerWantsMoreHeadline`; the manager bids again with `TransferTerms.player_weekly_wage` / `player_contract_years`. Hopeless: the existing "player refuses" answer, no demand. |
| Preview (`player_outlook`) | The same judgement, so the screen and the bid cannot disagree: keen / hesitant / unlikely, his standard terms and what he would ask. |
| An AI club's bid for the manager's player | Not generated when he would not go; checked again when the manager accepts (`be.error.transfers.playerRefusesMove`, nothing mutated). A hesitant player comes at the wage he asks if it is within 30 % of the standard; his contract is written at that wage. |
| AI-to-AI sweep (`evaluate_transfer_market`) | The same judgement inside `acceptable`, from squads read once per sweep (`SweepConsent`); the wage he comes for is the wage the board is asked about and the contract is signed at. |
| Registration of a deal that waited for the window | Re-checked on the day: only an outright refusal voids it (`RegistrationBlocked`, the existing inbox notice). |
| Loans, free agents | Unchanged. |

### Contract negotiated

`TransferTerms` gained `player_weekly_wage` and `player_contract_years` (both `serde(default)`, 0 = the buyer's
standard terms, which is exactly what every transfer used before). They ride on the offer, so they survive a
registration date months away, and `execute_transfer_with_terms` writes the contract from them
(`buyers_agreed_terms`). The board's wage policy is asked about this wage. No migration: offers are stored as JSON.

### Calibration

`tests/paid_transfer_calibration_tests.rs` offers the standard terms from clubs of five standings, at four
levels of ability (a club's squad is `50 + reputation/40`). See §5 for the table and the properties asserted.

## 3. Delegated recruitment

`Duty::PlayerRecruitment` ("Player recruitment", every 3 days, inside the existing `run_after_day` of
`process_day_common`, so a fast-forward gets it for free). `staff_system/delegation_recruitment.rs`.

| Rule | How |
|---|---|
| One transfer engine | The assistant calls `make_transfer_bid_with_terms` — the manager's own path. The selling club, the counters, the cool-down, the board and **the player** judge him exactly as they judge the manager. Nothing in the module lists, offers or releases a player |
| Needs, not appeal | A position the assistant's own review (`assistant::recommendations`, `WeakPosition`) calls weak, a candidate who improves the first cover by ≥ 3 points (or a body for a thin group), senior, known to the club (his division or a scout's report: `player_knowledge`), ≤ 31 (GK 34), squad under 27 seniors |
| Price | He reads the club's asking price (`minimum_acceptable_fee_at`), opens at 95 % (above the 88 % counter floor, so he gets a counter and not a flat no), pays up to 110 %, never chases a club asking over 150 % of value, never more than 60 % of the transfer budget on one player |
| Authorisation | Every bid is passed through `authorize(…, Spend::Transfer { fee, weekly_wage, years })` at the fee and wage **of that bid**. `Auto` → he bids; `NeedsApproval` → a `SignPlayer` proposal and nothing is spent; `NotDelegated` → nothing. A fee past his own price judgement is also a proposal |
| Safe default | `authorize` now treats a transfer ceiling of **zero as "every acquisition is the manager's"**, fee or not (before, a fee-free signing would have passed). The default stays 0 |
| Time | One round per visit, 3 days apart. Where a negotiation stands (`ActiveApproach`: next fee, the wage and length the player asked for, rounds) is in `DelegationState.recruitment` |
| No loops | `ShelvedTarget` (club refused 60 d, player refused 90 d, not possible 30 d, manager declined 90 d, proposal unanswered 35 d, signed 365 d); at most 2 negotiations open, 3 new approaches in 30 days, one new target per visit, 5 rounds, a negotiation older than 45 days is dropped |
| Explainable | Log entries and proposals carry the position, player, fee, wage and years (`delegation.log.recruit*`, `delegation.proposal.signPlayer`) |
| Approval | `resolve_proposal(accept)` makes the bid at the approved fee, wage and length. A counter above what was approved comes back as a new proposal. Declining shelves him for 90 days |
| Persistence | `DelegationState.recruitment` is part of the one `staff_delegation` JSON row; every field `serde(default)`. No migration. The rollover touches nothing in it |
| Existing saves | `settings` has no row for the new duty: it reads as not delegated, asks for approval. A save in `Full` therefore reads as `Custom` until the manager hands recruitment over |

Presets: `Approval` and `Full` now include the new duty. With the default zero ceiling both only ever produce proposals.

## 4. Behaviour changes to existing code (deliberate)

* `an_ambitious_player_refuses_to_step_down_unless_promised_a_way_out` became
  `…wants_something_for_stepping_down_a_promise_or_a_bonus_will_do`: without a promise or bonus he now asks for a
  better contract (`WantsMore`) instead of a flat refusal; the promise and the bonus still win him over.
* `free_agent_signings_leave_scheduled_transfer_obligations_alone` meets the wage the player asks before its
  assertions (it is about instalments, not willingness).
* `transfers.transferFeedbackPlayerRefusesDetail` no longer says "without a promise or a signing bonus" (12 locales).
* `transfer_make_bid` (MCP) takes the optional `player_weekly_wage` and `player_contract_years` and reports what
  the player asks, otherwise an agent could not meet him.

## 5. Calibration

### The player's side (`paid_transfer_calibration_tests`)

One player offered the standard terms by a club of each standing (a club's squad is `50 + reputation / 40`; the
player is a regular wherever his rating puts him). Rows are the club he leaves, columns the club that bids,
from the smallest (80) to the largest (900) reputation; K = keen, H = hesitant, U = unlikely.

| Player | Leaves | 80 | 200 | 400 | 650 | 900 |
|---|---|---|---|---|---|---|
| ovr 58 | 900 | U | U | U | H | K |
| ovr 58 | 650 | U | U | H | K | K |
| ovr 58 | 400 | H | K | K | K | K |
| ovr 58 | 200 | H | K | H | K | K |
| ovr 58 | 80 | K | K | H | K | K |
| ovr 66 | 900 | U | U | U | H | K |
| ovr 66 | 650 | U | U | H | K | K |
| ovr 66 | 400 | U | H | K | K | K |
| ovr 66 | 200 | H | K | K | K | K |
| ovr 66 | 80 | K | K | K | K | K |
| ovr 74 | 900 | U | U | U | H | K |
| ovr 74 | 650 | U | U | H | K | K |
| ovr 74 | 400 | U | H | K | K | K |
| ovr 74 | 200 | H | K | K | K | K |
| ovr 74 | 80 | K | K | K | K | K |
| ovr 84 | 900 | U | U | U | H | K |
| ovr 84 | 650 | U | U | H | K | K |
| ovr 84 | 400 | U | H | K | K | K |
| ovr 84 | 200 | H | K | K | K | K |
| ovr 84 | 80 | K | K | K | K | K |

Read-offs: a player of the biggest club (900) says no to anything at or below a 400 club and is only hesitant
about the 650 one, whatever his ability; at a 650 club he refuses the two smallest, hesitates over a 400 club and is
keen on bigger ones; at a 400 club the elite player is unlikely to drop to a minnow and hesitant over a 200 club;
a player of a minnow is keen on nearly everyone (a step up is welcome). The only irregularity is the 58-rated
player at the two smallest clubs, who is hesitant about a 400 club while keen on a 200 one: its squad is deep
enough that he would sit on the bench, and the part he would play counts. The asserted properties (the full table is printed with
`--nocapture`): among buyers no bigger than his club, a bigger buyer is never less attractive; an elite player
at a giant is unlikely to join a minnow and keen on a step up; a lateral move is never unlikely; pay lifts a
hesitant player to keen and never lifts a hopeless one.

### The AI market (`season-harness/tests/measure_ai_market.rs`, ignored measurement)

Gate world, seeds 1-3, three seasons, the same counters read off the transfer logs, run on the base commit and on
this branch: **identical to the last digit** (cumulative moves 26 / 55 / 85, 18 / 40 / 62, 29 / 51 / 80; mean fee
about 0.8-1.1 M; abroad and up-the-ladder counts unchanged). The market was already restricted to buyers no more
than 150 reputation under the seller, a band in which the player is keen, so consent is a guard on the AI path
(it fires when a squad, a role or a debt makes a move implausible) and not a throttle: no market test needed its
tolerances changed and the existing realism tests (`transfers_tests::realism`, `deadline_tests`) pass untouched.

## 6. Tests

| # | Scenario | Test |
|---|---|---|
| 1 | Modest club, elite player of a prestigious club | `paid_transfer_consent_tests::a_modest_club_cannot_buy_an_elite_player_just_by_paying` |
| 2 | Selling club accepts, player refuses (with the control that proves the clubs agreed) | `…::the_selling_club_accepts_but_the_player_refuses`, `…::credible_circumstances_make_a_step_down_possible`, `…::a_player_can_refuse_to_join_a_club_that_bids_for_him` |
| 3 | Player keen, selling club refuses | `…::the_player_is_keen_but_the_selling_club_refuses` |
| 4 | Realistic transfer after negotiation | `…::a_realistic_transfer_succeeds_after_negotiation` |
| 5 | Salary, length and role | `…::the_player_negotiates_his_wage_and_length`, `…::the_part_he_would_play_counts` |
| 6 | Financially impossible deal | `…::a_financially_impossible_deal_is_refused_untouched` |
| 7 | Competing AI interest, no duplicate | `…::competing_ai_interest_never_duplicates_a_transfer` |
| 8 | Assistant recruits within limits | `delegation_recruitment::tests::the_assistant_signs_a_suitable_player_within_the_limits_he_was_given`; `season-harness/tests/delegated_recruitment.rs` on a generated world |
| 9 | Over the limits: proposal, nothing executed | `…::a_deal_above_his_limits_is_proposed_not_executed`, `…::with_the_default_ceiling_nothing_is_ever_executed`, `…::a_duty_that_asks_for_approval_proposes_even_within_the_ceiling`, `delegation_policy::…transfers_are_the_managers_until_a_fee_ceiling_is_set` |
| 10 | Manager declines, no repeat | `…::a_declined_proposal_is_not_repeated` |
| 11 | Fast-forward | `fast_forward::a_fast_forward_processes_delegated_recruitment_like_the_days_played_singly` (same world as single days, memory included), `…::a_signing_proposal_stops_a_run_that_stops_for_decisions`, `…::a_target_date_run_records_signing_proposals_without_executing_them` |
| 12 | Save/reload during a pending transfer | `db/tests/pending_transfer_reload_tests` (the agreement with the player's own wage registers identically after a reload; the assistant's memory survives two reloads); mid-window and post-rollover reloads in `delegated_recruitment` |
| 13 | Multi-season market | `delegated_recruitment` (season + rollover + 30 days), `measure_ai_market` (3 seasons x 3 seeds), existing `season_lifecycle` |
| 14 | Free agents and obligations do not regress | `free_agent_talks_tests`, `stepping_stone_tests` and `deal_terms_tests` pass; two assertions were adapted (§4) |

Also: the assistant never sells or lists (`the_assistant_never_sells_or_lists_a_player`), recruits nothing when the
squad has no need, when the duty is off, or without an assistant; unit tests of the judgement
(`transfers::consent::tests`); the front-end form (`TransferBidModal.test.tsx`, `bidTerms.test.ts`).

## 7. Limits and what is not done

* **Response delay of the player to the manager's bid.** A bid is still answered the same day. The player's
  extra step is an immediate "he wants other terms", not a wait. The delegated path is spread over days (one round per
  visit); a manual bid is not.
* **Role promises.** The player's expected place is weighed, but a manager cannot promise a role and be held to it.
* **Free agents are not recruited** by the assistant (the free-agent talk is a different, delay-based workflow). The
  duty buys under contract only.
* **Loans** are not subject to the player's consent and are not recruited by the assistant.
* The assistant reads the selling club's asking price (`minimum_acceptable_fee_at`) to open at; that is a convenience
  of the single engine, not information a human manager has.
* An existing save in `Full` mode reads as `Custom` until recruitment is handed over.
* Locale strings for the 11 non-English locales were written without a native review.
* The gate world's AI market is not changed by the player's consent (§5); the effect on the 12-league FM24 world
  was not measured.
* **M12**: the real `~/apfm-match-lab` is not available here. A stub kept outside the repository
  (`/home/apfm-match-lab`, never committed) let the workspace compile; the six tests that execute the engine
  (`spatial_runner::tests::m12_execution_projects_a_deterministic_coherent_match_report` and the five
  `commands::spatial_match` M12 tests) cannot pass against it and were neither run as evidence nor touched. Nothing in
  `spatial_*`, the match viewer, replay persistence or Cargo paths was changed. Run `scripts/apfm/run-m12-tests.sh` on
  the Mac before merging.
