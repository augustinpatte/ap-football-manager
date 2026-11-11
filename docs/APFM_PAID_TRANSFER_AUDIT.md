# APFM — Paid transfer audit (Run E, Part 1)

Written **before** any code change, from the branch `integration/gameplay-runs-a-d` (head `05eec51`).
Every statement below was read in the code; file names are given so it can be re-checked.
The implementation that answers the gaps is described in `APFM_PAID_TRANSFER_REALISM.md`.

## 1. The path of a paid transfer

```
React (TransferBidModal, PlayerDealWorkspace, TransfersPlayerOffers)
  → services/transfersService.ts (invoke)
  → commands/transfers.rs::make_transfer_bid / respond_to_offer / counter_offer   (one `_internal` fn each)
  → ofm_core::transfers::{make_transfer_bid_with_terms, respond_to_offer, counter_offer}
  → execute_transfer_with_terms  → terms::{settle_cash, apply_budgets, record_obligations}
  → Game (in memory) → db::player_repo (offers as JSON), transfer_obligations table, cash journal
MCP: tools_impl/transfers.rs → the same `commands::transfers::*_internal` functions (no second copy).
AI clubs: transfers/market.rs::evaluate_transfer_market (daily sweep) → execute_transfer.
```

There are exactly three call sites of the money-moving `execute_transfer*` functions: the user's bid
(`bids.rs`), the user's answer to an incoming offer (`bids.rs`), the registration of an agreement that
waited for the window (`execution.rs::process_pending_transfer_registrations`) and the AI sweep
(`market.rs`). MCP and React reach the first two through the same Rust functions, so **no frontend
or MCP route bypasses a Rust validation** (the MCP `transfer_*` tools are thin adapters).

## 2. Answers to the audit questions

| Question | Original behaviour | Status |
|---|---|---|
| Can clubs accept, reject and counter fees? | **Yes.** `make_transfer_bid_with_terms` compares the bid with `minimum_acceptable_fee_at` (value × listed/contract/importance/openness/window-phase multipliers, × the instalment/sell-on price factor of the terms). At or above: accepted. Between `counter_floor` (88–94 % of it, by round) and it: `CounterOffer` with a rounded suggested fee. Below, or past `NEGOTIATION_BREAK_OFF_ROUND`: rejected, talks broken off, cool-down before re-bidding. | Working. Not rebuilt. |
| Can the user counter an incoming AI offer? | **Yes.** `counter_offer` against `buyer_counter_offer_ceiling`; accepted / countered / broken off. | Working. |
| Do offers take in-game time? | **Partly.** A *bid by the user is answered the same day* (no response delay, unlike free-agent talks which take 1–3 days). Time enters in three places: an agreement made while the window is closed becomes `PendingRegistration` and executes on the registration date; counters/rejections open a re-bid cool-down (`REBID_COOLDOWN_DAYS`, shortened near the deadline); offers go stale (`expire_stale_transfer_offers`) and all open talks end at the deadline. AI-to-AI deals are instantaneous inside the daily sweep. | Same-day answer kept (see §4). |
| Does the player negotiate a separate employment contract? | **No.** `buyers_standard_terms` calls `standard_contract_terms(player, buyer, today, 0)`: the wage is `expected_wage` at the buyer, the length `expected_contract_years`. `TransferOffer::wage_offered` exists but every writer sets it to 0. The player has no counter-offer, no wage or length to bargain over, and the manager cannot offer more or less than the standard terms. The only money that reaches the player is the optional signing bonus. | **Missing.** |
| Do salary, contract length, bonuses, squad role affect acceptance? | **Bonus only, and only inside one rule** (`terms::player_refuses_move`, below). Wage and length are fixed, so they cannot matter. Squad role / expected playing time are not looked at at all for a purchase. | **Missing** (bonus partially present). |
| How does `player_refuses_move` work? | `terms.rs:97`. Returns `false` unless `wants_stepping_stone(player)` (young, ambitious, room to grow — `promises.rs`). If he does want one: refuses when `buyer.reputation + 100 < owner.reputation` **and** the bid carries neither a stepping-stone promise nor a signing bonus ≥ 26 weeks of his reference wage. Called in one place, `make_transfer_bid_with_terms`, **after** the clubs agreed; result is a non-terminal `Rejected` with `transfers.transferFeedbackPlayerRefusesHeadline`. | Working but narrow: a 28-year-old star at a top club has **no** veto at all. |
| How do reputation and sporting ambition affect the player? | Only through the rule above (reputation gap > 100, youngsters). For the *seller's* price, `player_move_openness_score` (morale, appearances, contract, destination reputation gap, listed) lowers the asking price; it never blocks a move. Free agents, by contrast, are judged by the Run C appeal score (`contracts/free_agent_talks/appeal.rs`: wage, club standing vs player standing, ambition, role, length, desperation, promise). | **Missing for contracted players**; the model exists for free agents. |
| Do AI clubs obey the same constraints? | **Financial: yes.** The sweep requires `transfer_budget ≥ price`, `finance ≥ price`, board wage policy (`buyer_can_pay_standard_wage`), `SigningBooks::can_carry`, not overstretched, squad floor of the seller. **Sporting: buyer side only** — a need in the position group, a rating gain, `buyer_has_genuine_interest` (buyer reputation at most 150 under the seller's, softened for promoted clubs and clubs with a hole near the close) and `willing_to_sell` for the seller. **The player is never consulted** in an AI-to-AI transfer, nor when an AI club bids for the user's player and the user accepts (`respond_to_offer`): no `player_refuses_move`, no appeal. | **Asymmetric**: the user is vetoed by a player in one narrow case; AI clubs and incoming offers are not vetoed at all. |
| Does any route bypass validation? | Reviewed: bid, accept, counter, registration, AI sweep, MCP. Findings: (a) `respond_to_offer` / `counter_offer` / registration run **no player-willingness check**; (b) the *registration* of an agreement made while the window was closed re-checks funds and the board but not the player, months later; (c) nothing records an agreement as "the player has agreed", so the check cannot be replayed at registration. Money, squad floor, loan conflicts, own-player bids, duplicate pending registrations and the transfer budget are all validated in Rust and are consistent. No route trusts the frontend. | Gaps (a)–(c). |
| Duplicate signing / repeated payment | Guarded: `has_pending_transfer_registration`, `player_has_active_or_pending_loan`, `moved_player_ids` in the sweep, `finalize_successful_transfer_offer` withdraws every other pending offer, one atomic cash posting (`settle_cash`). | Working. |
| Instalments, sell-on, bonuses, history | `terms.rs`, `TransferObligation`, `record_movement`, `CompletedTransfer` log, transfer news. Persisted (obligations table; offers as JSON). | Working. Preserved. |

## 3. Delegation (Run A) — what exists and what is missing

* `staff_system::delegation_policy::authorize(&delegation, duty, Spend::Transfer { fee, weekly_wage, years })`
  exists, is unit-tested, and `DelegationLimits::max_transfer_fee` defaults to `0`.
* **Nothing calls it with a `Spend::Transfer`.** `grep Spend::Transfer` finds only the test and the
  definition. No duty buys a player; `Scouting` only sends scouts; the assistant's recommendations
  (`assistant.rs`) name weak positions but never act on the market.
* A zero fee ceiling is only "safe" because of `fee > ceiling`: **a fee-free signing (fee 0) with a
  zero ceiling would pass** as `Auto` when the wage is under the player-wage ceiling. Nothing exercises this
  yet, but it must be closed before a caller exists.
* Proposals already stop the fast-forward (`DecisionKind::DelegationProposal` in
  `fast_forward::pending_decisions`), and a user's pending bids count at the deadline
  (`transfer_deadline_decision`). Both therefore cover assistant negotiations without change.
* The delegation state is one JSON row (`staff_delegation`), every addition `#[serde(default)]`.

## 4. What this run changes

| Gap | Resolution |
|---|---|
| No player-side judgement of a paid move | `transfers/consent.rs`: the Run C appeal score, extended with the facts of a player who already has a club (see the realism document). Applied to the user's bids, to incoming offers (when generated and when accepted), to the AI sweep and to registration. |
| Standard terms only | `TransferTerms` gains the player's own wage and length; the player may counter with his ask. |
| Role / playing time ignored | Part of the appeal (place in the new squad against place in the current one). |
| Narrow `player_refuses_move` | Subsumed: its two ways out (a promise, a bonus) are terms of the appeal. |
| Zero ceiling and fee-free signings | `authorize` treats a zero fee ceiling as "every acquisition is the manager's". |
| No automated recruiter | `Duty::PlayerRecruitment`, run by the assistant inside `process_day_common` through the ordinary bid path. |

Not changed on purpose: the same-day answer to a user bid (adding a response delay would add a second
state machine to a path that already has a registration date and cool-downs; the delegated path is
spread over days by its own cadence), the seller's valuation, instalments/sell-on/bonus, loans.
