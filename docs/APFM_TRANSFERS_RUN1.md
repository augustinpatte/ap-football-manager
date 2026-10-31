# APFM — Run 1: transfers, contracts, loans, deadline day

What this run added on top of the transfer system OFM already had, what it deliberately did not
touch, and what is still missing. Written against `4e5dd35`.

## Already in the codebase (verified, not rebuilt)

Permanent bids with counter-offers and break-off rounds, incoming AI offers and user responses,
loan offers with wage contribution, buy options and returns, free-agent signing, renewals with
wage-policy checks, delegated renewals, contract expiry, pending registration when the window is
closed, budget and squad-floor checks, and the AI market sweep (needs, targets, surplus sales,
rebid cooldowns). All of it persists through the existing `Player` JSON columns.

## Added

### Stepping-stone promise
- `domain::player::SteppingStonePromise`, stored in `PlayerMoraleCore.stepping_stone_promise`.
  It rides the existing `morale_core` JSON column, so **no migration** and old saves read `None`.
- Made when signing a free agent (`offer_free_agent_contract_with_promise`). The player takes a
  10% lower wage in exchange. Refused with `be.error.contracts.promiseNotWanted` for a player who
  has no use for it (over 27, or less than 3 points of potential above his rating).
- Terms (`ofm_core/src/transfers/promises.rs`): after 180 days and for 3 years, an offer is
  *suitable* if the bidder's reputation is at least the club's + 50 and the fee is at least 80% of
  market value.
  - Refusing a suitable offer → `Breached`: manager trust −30, morale −15, renewal talks blocked
    180 days (reusing the renewal session's own block), inbox message.
  - Selling to a suitable bidder → `Fulfilled`, inbox message.
  - Reaching the end unbroken, or the player leaving by other means → `Lapsed` (trust +5 in the
    first case), processed once per day and idempotent.

### Deadline day
- `transfers::transfer_deadline_summary` (command `get_transfer_deadline_summary`): phase, days
  left, offers awaiting an answer, counters received, agreed deals awaiting registration, and the
  last three days of completed deals.
- `transfers::expire_negotiations_at_deadline`, run at the end of the closing day's processing:
  every still-`Pending` offer or loan approach is withdrawn and the user gets one inbox message.
  Agreed deals (pending registration) are untouched. Offers that arrived that same day are spared,
  since the user has not yet seen them; they lapse by the normal staleness rule.
- UI: `TransferDeadlinePanel` above the Transfers tab, shown in the final week or whenever
  something needs an answer.

### Deal terms: signing bonus, instalments, sell-on clause (second pass)
- `domain::transfer_terms::TransferTerms` rides on `TransferOffer.terms` (serde default, so older
  saves read a plain fee) and is kept through a counter-offer and through a closed window.
- **Signing bonus** (free agents and permanent bids). A free agent weighs `wage + bonus / weeks of
  contract` against what he expects; the board's wage policy is still held to the wage alone. The
  bonus is paid on signing out of cash *and* transfer budget (`CashKind::SigningBonus`, counted in
  season expenses), and cannot exceed the wages of the whole contract.
- **Instalments** (up to 4, one a year apart). The first part (which absorbs the rounding
  remainder) and any bonus are paid on completion; later parts become `TransferObligation`s in a new
  `transfer_obligations` table (migration `v062`), paid once on their due date by the daily turn,
  marked paid in the same step that moves the money, and dropped from the save 120 days later. The
  buyer's transfer budget is debited the whole fee on completion; the seller's is credited as each
  payment arrives.
- **Sell-on clause** (up to 30% of the next fee). Recorded as an obligation held by the buyer for
  the seller; settled in full from the holder's cash, in the same atomic posting as the next sale's
  fee, then spent. A sale that would trigger a clause the seller cannot fund is refused before the
  offer is marked agreed.
- The seller prices the terms: +4% per extra instalment, -1% per 3% of sell-on.
- UI: a "deal terms" block in the bid form and a bonus field in the free-agent form, with the
  cash-now / instalments / bonus lines in the projection.

### Player consent on permanent transfers
Once the clubs agree, an ambitious player (young, with room to grow) refuses to step down to a club
whose reputation is more than 100 below his current club's, unless the terms carry the stepping-stone
promise or a bonus worth at least 26 weeks of his usual wage. The refusal is a non-terminal outcome
with its own feedback, so the manager can come back with a promise or a bonus. A promise given on a
permanent bid is recorded when the deal completes, including one that waited for the window.

### Inbox actions
The per-player transfer-interest message now carries an `accept` / `reject` choice on the offer it
describes. It runs the same `respond_to_offer` as the Transfers tab (squad floor, wage policy,
registration date). Answering twice does nothing; an offer already gone closes the action with an
explanation; a refusal leaves the action open with the reason.

## Not done, and why

| Item | Status |
|---|---|
| Promise on loans | Loans are decided by clubs only. |
| Promise effect beyond the player | Outcomes touch that player's trust, morale and renewal willingness. No club-wide "agent trust". |
| Instalments / clause on incoming AI offers | AI clubs bid a lump sum; only the user's bids carry terms. |
| Sell-on settled by instalment | A clause is paid in full at the sale, never spread. |
| Inbox actions for loan offers and counter-offers | Only permanent incoming offers can be answered in place. |
| MCP tools for the new terms / deadline summary | Not added; MCP bids and signings pass plain terms. |
| Agentic-FC reference | Studied as inspiration only; no code or runtime imported, no attribution needed. |

## Verification environment

The M12 engine crate `apfm-match-lab` lives outside this repository (`../../../../../apfm-match-lab`
from `ofm_core`). It was absent in the cloud container, so Rust was built against a throwaway stub
kept **outside the repo**. Tests that need the real M12 simulation (`spatial_runner`,
`commands::spatial_match`) fail against that stub and are not evidence either way. The Tauri
window itself cannot run in the container.
