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

## Not done, and why

| Item | Status |
|---|---|
| Installments, sell-on clauses, signing bonuses | Not in the data model (no field anywhere); adding them touches fees, finances and persistence. Deferred. |
| Promise on permanent bids / loans | Only free-agent signings give the player a say today; bids are decided by clubs only. |
| Promise effect beyond the player | Outcomes touch that player's trust, morale and renewal willingness. No club-wide "agent trust". |
| Answering an offer straight from an inbox message | Existing inbox messages are informative; the Transfers tab (and the new panel, which lists the offers) is where answers happen. |
| MCP tool for the deadline summary / promise | Not added; `offer_free_agent_contract` through MCP passes `promise = false`. |
| Agentic-FC reference | Studied as inspiration only; no code or runtime imported, no attribution needed. |

## Verification environment

The M12 engine crate `apfm-match-lab` lives outside this repository (`../../../../../apfm-match-lab`
from `ofm_core`). It was absent in the cloud container, so Rust was built against a throwaway stub
kept **outside the repo**. Tests that need the real M12 simulation (`spatial_runner`,
`commands::spatial_match`) fail against that stub and are not evidence either way. The Tauri
window itself cannot run in the container.
