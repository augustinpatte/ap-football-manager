# APFM I2.1: transfer integrity

Branch `deepseek/i2-1-transfer-integrity`, based on the I2 integration commit `73db8d43`.
Companion: [`APFM_I2_1_VALIDATION.md`](APFM_I2_1_VALIDATION.md) (commands, red-then-green evidence,
career results, verdict).

Scope: the backend transfer, negotiation, contract and registration code, its Rust tests and
backend docs. No React, translation, match-engine, M12, formation-AI or tactical file changed.
Frontend issues remain tracked for I2.2 (see §9).

## 1. What I2 left broken

I2 gave one senior-list predicate to in-game and global-market **purchases**, but the other routes
that put a senior on a club's list still read the live list only. The phase also left three review
items and one financial asymmetry.

| # | Defect | Cause | Player-visible effect |
| --- | --- | --- | --- |
| 1 | A free-agent talk could take the place promised to an agreed purchase. | `contracts::free_agent_talks::workflow` used `squad_capacity(..).blocks_signing`, which counts only registered seniors. | With 29 seniors and a purchase agreed and waiting to register, a free agent could sign into the 30th place; when the purchase fell due it failed and was withdrawn. |
| 2 | The global market exempted free agents from the reservation. | `external_registry::squad_blocker` passed `promised = 0` when `free_agent`. | Same lost place through the Global Market tab. |
| 3 | An AI club could sign a free agent into a promised place. | `squad_floor::sign_free_agent_for` never asked the capacity predicate. | The user's agreed deal failed to register because an AI club took its place first. |
| 4 | A free agent's signing bonus was judged against raw cash. | `validate_affordable` called `ensure_bonus_affordable`, which ignores the fees of deals agreed and waiting to register. | A club could pass the bonus check, then have to fund both the bonus and a pending fee from the same cash. |
| 5 | The legacy external preview re-implemented the negotiation and had drifted. | `preview_external_recruitment_with_offer` duplicated the pricing, identity, squad and finance checks instead of calling `negotiate_external_transfer`. | The preview could say "signable" where signing refused (for example a player who refuses the move), and its blocker ordering could differ. |
| 6 | `respond_to_offer` / `counter_offer` reached the limit without a boundary test. | I2 item 3, no test. | A regression on either command would have been invisible. |
| 7 | Pending **loan-ins** still take no place. | `pending_incoming_purchases` counts purchases only; pre-existing. | A loan and a purchase can both be promised the last place. Not fixed here (§9). |

## 2. The rule, in one place

`transfers::registration::signing_place_refusal(game, team_id, incoming_player_id)` is now the one
predicate every signing route asks:

```rust
pub(crate) fn signing_place_refusal(
    game: &Game,
    team_id: &str,
    incoming_player_id: &str,
) -> Option<String> {
    let promised = pending_incoming_purchases(game, team_id, incoming_player_id);
    senior_place_refusal(game, team_id, promised)
}
```

* The reservation is **derived** from the saved offers: a player away from the club with a
  `TransferOfferStatus::PendingRegistration` offer **from** that club. There is no new field, no
  second source of truth and no schema or migration change.
* A status transition releases the place without extra code: registration (`Accepted`), a failed
  registration (`Withdrawn`) and a released/voided offer stop matching.
* `incoming_player_id` is excepted, so a deal that is registering does not block itself through its
  own pending offer. The exception is essential: during `finalize_successful_transfer_offer` the
  deal still carries its `PendingRegistration` offer while the player is being added.
* `senior_place_refusal` remains the one capacity algorithm: refuse when
  `free_places <= promised`. It never exceeds the 30-senior limit and never subtracts a promise
  twice.

## 3. Routes and what changed

| Route | Entry point | Before | After |
| --- | --- | --- | --- |
| In-game bid / negotiation at agreement | `transfers::execution::ensure_buyer_can_pay_standard_wage` | Counted pending purchases through `ensure_place_for(..., promised)`. | Same rule, through `signing_place_refusal(.., &player.id)`; the player's own offer is excepted. |
| In-game registration | `transfers::execution::buyers_contract_terms` / `ensure_place_for` | `promised = 0` at registration, so the executing deal did not count itself but **other** pending deals were ignored. | `signing_place_refusal(game, buyer, &player.id)`: other promises are respected; this deal is excepted. |
| Free-agent talk, offer | `contracts::free_agent_talks::workflow::submit_free_agent_offer` | `ensure_room_in_the_squad` read the live list. | `ensure_room_for(game, club, player_id)` → shared predicate. |
| Free-agent talk, completion | `contracts::free_agent_talks::workflow::complete_signing` | Live list only; a place could be promised while the player took days to answer. | Same shared predicate, so the talk lapses instead of stealing the place. |
| Free-agent signing bonus | `workflow::validate_affordable` | `ensure_bonus_affordable(team, bonus)`: raw cash. | `fee_and_bonus_block_after_pending(game, team, 0, bonus)`, the same pending-aware test as a purchase. |
| Global market, any signing | `external_registry::squad_blocker` | Reserved places for contracted players only. | `signing_place_refusal` for free agents and contracted players alike. |
| Global market preview | `preview_external_recruitment_with_offer` | A second implementation of the same checks. | Delegates to `negotiate_external_transfer` with `ExternalNegotiationRequest { fee: offer, ..Default }` and maps the outcome. The preview now includes club rejection, player refusal, contested wage, fee + bonus affordability and the same blocker order as an attempt. |
| AI squad planning | `squad_floor::sign_free_agent_for` | No capacity gate. | `signing_place_refusal(game, &team.id, "")` before the market scan; returns `None` on refusal, the function's existing "nobody suitable" result. |

No second transfer implementation was added and no public signature a frontend depends on changed.
`fee_block_after_pending` became dead once the preview delegated and was removed; its behaviour is
`fee_and_bonus_block_after_pending(.., bonus = 0)`. `pending_incoming_purchases` and
`senior_place_refusal` are no longer re-exported outside `transfers` because only
`signing_place_refusal` should be called from other modules.

## 4. Boundary behaviour

The reservation is only meaningful at the boundary; these are the outcomes, all exercised through
production entry points (an in-game bid, `submit_free_agent_offer`, `preview_external_recruitment`,
`recruit_external_player`, `bring_in_one`, `respond_to_offer_internal`, `counter_offer_internal`,
`process_pending_transfer_registrations`).

| Seniors | Agreed purchase waiting | Event | Outcome |
| --- | --- | --- | --- |
| 29 | none | bid for a player | agreed; the deal takes the last place. |
| 29 | one | bid for another player | refused with `be.error.freeAgents.squadFull` (parameters `count=29&max=30&release=0` for this state); no offer is created. |
| 29 | one | free-agent offer | refused with the same key; no talk is opened. |
| 29 | one | global-market preview / signing (free agent or contracted) | preview blocker and signing refusal agree; nothing mutates. |
| 29 | one | AI club looks for a free agent | signs nobody. |
| 30 | one | bid for another player | refused; the list stays 30 and the promise is not subtracted twice. |
| 30 | one | global-market free-agent signing | refused; the list stays 30. |
| 29 | one | the deal registers | registers into its own place: the exception works, list 30. |
| 28 | one | a senior leaves, then the deal registers | registers; list 29. |
| 29 | one | a senior leaves, then a free agent is offered/signed | accepted; the place is genuinely free. |
| 29 | one | a purchase is withdrawn at its due date | its place is released; the next bid is agreed. |
| 29 | one | save and reload | the reservation survives; the next bid is still refused. |
| 30 | none | `counter_offer` at a fee the buyer refuses | the rejection is recorded; no place is needed to refuse. |

`respond_to_offer` and `counter_offer` are refused **before** mutation when the buyer cannot house
the player; a serialized before/after comparison shows the career unchanged, including the offer's
status and fee.

## 5. Financial consistency

The same reservation idea already governed cash, transfer budget and wages in I2; this phase closes
the free-agent gap and keeps the guarantees:

* `pending_fee_commitments` still holds cash and budget for every agreed purchase; the free agent's
  bonus is now tested **with** the pending fees by `fee_and_bonus_block_after_pending(.., 0, bonus)`.
* A failed offer mutates nothing: the free-agent, in-game, global-market and command tests compare
  either the serialized game or the specific state (no offer, no talk, no team change, no cash
  movement).
* A talk that can no longer be afforded lapses at completion (existing behaviour) instead of
  overdrawing; a talk whose place was promised in the meantime lapses for the same reason.
* Commitments are derived from `PendingRegistration` offers, so they cannot be left behind by a
  refusal, a withdrawal or an expiry. The withdrawal test proves the released place is usable; the
  companion free-agent test proves the released fee makes a previously refused bonus affordable.

## 6. Performance

The reservation is a linear scan of `game.players` (and each player's offers) that runs only when a
signing is attempted, or when an AI club considers a free agent — never per match minute, per day or
per render. A generated career holds about 3.7 k players, so one scan is a few thousand predicate
checks: smaller than the market scan the AI performs immediately afterwards, and paid only by a club
that is about to sign. `squad_capacity` is the same linear count the old live-list check made, so the
new cost is the pending-offer filter, not the capacity rule. No collection is allocated; only a
refusal builds its key string.

The three-seed, ten-season career with invariants on took **145.27 s** of test time (about 48.4 s
per seed) on this host, against I2's recorded roughly 45 s per seed on the same machine. The
direction and size of the change are what matter: AI clubs now skip free-agent signings they would
previously have made into a promised place, so the two runs are not expected to be output-identical.
No regression in career length or invariants was observed. A precise instruction-count comparison
was not run; see validation §7.

## 7. Compatibility

| Contract | Decision |
| --- | --- |
| Save format | **Unchanged.** No field added, removed or reordered; no `#[serde(default)]` needed. The reservation is derived from existing `TransferOfferStatus::PendingRegistration` offers. |
| SQLite schema / migrations | **Unchanged.** Transfers are part of the saved game document; no repository or migration edit. |
| Tauri IPC / service wrappers | **Unchanged.** `preview_external_recruitment` and `recruit_external_player` keep their signatures and are kept for the existing frontend services. Their behaviour is now consistent with the negotiation path. |
| Old saves | A save from I2 loads and reserves the same places automatically, because the offers are already stored. No repair pass is needed. |
| Translation keys | No key added. Refusals reuse `be.error.freeAgents.squadFull?count=&max=&release=`; the mid-talk failure reuses `FreeAgentTalkReason::ClubCannotAfford`. |

## 8. Tests added or rewritten

Twenty-one tests are new; one characterization test is rewritten to the fixed rule.

| Area | Tests |
| --- | --- |
| In-game purchase and registration (`transfers_tests/squad_places.rs`, new module) | last free place; refused when promised; full list with a pending deal; own registration not self-blocked; registration after a place frees; free-agent talk refused/accepted; withdrawn deal releases its place; save/reload. |
| Global market (`transfers_tests/external.rs`) | free agent cannot take a promised place (rewritten from the I2 limitation pin); full list with a pending deal; preview and attempt agree and the signing works after a place frees; the legacy one-click preview and the attempt report the same player refusal. |
| Free-agent talks (`tests/free_agent_talks_tests.rs`) | offer refused while a place is promised; mid-talk lapse when the place is promised before the answer; bonus judged against pending fees and released when the deal is withdrawn. |
| AI (`squad_floor.rs`) | a club with a promised place signs no free agent; a full club signs none; the same club with a free place signs one. |
| Commands (`commands/transfers.rs`) | `respond_to_offer` and `counter_offer` mutating refusals; a counter rejection still recorded with no place. |

## 9. Residual limitations

Ranked; none blocks the transfer-integrity fix.

1. **MEDIUM (pre-existing, named in I2 §6 item 7). A pending loan-in takes no senior place.**
   `pending_incoming_purchases` counts purchases. A club can therefore promise the last place to a
   purchase and to a loan-in at the same time. Loans and academy promotions also consume places
   without a capacity check. Out of scope here: changing loan registration changes AI and
   simulation outcomes beyond the reservation this phase was asked to add; it needs its own
   boundary matrix and baseline.
2. **MEDIUM (pre-existing). A mid-talk signing failure caused by the squad limit reuses
   `FreeAgentTalkReason::ClubCannotAfford`.** The talk lapses, which is correct; the reason's
   player-facing wording says money rather than places. A new reason needs a translation round
   (all 12 locales) and belongs with an I2.2 UI/localisation change, not a backend-only fix.
3. **LOW. The AI gate is placed in `sign_free_agent_for`, not in the academy-promotion branch of
   `bring_in_one`.** See item 1. The gate's failure result is the function's existing `None`, so a
   club that would once have signed a free agent into a promised place now leaves the group short
   until the purchase registers; the career runs show no invariant or world-health regression.
4. **LOW (pre-existing, I2 §6 item 4).** Duplicated negotiation validation inside D7B3
   (`validate_negotiation_terms`; the separate `signing_bonus_check` still calls
   `ensure_bonus_affordable` even though `fee_and_bonus_block_after_pending` now covers the pair).
   The preview delegation removes one copy of the **preview**, not these helpers. Epic #589.
5. **LOW. No production command cancels a `PendingRegistration` purchase.** Release happens on
   registration success (`Accepted`) or failure (`Withdrawn`); `expire_stale_transfer_offers` only
   touches `Pending`. The derived reservation is correct for every existing transition; a future
   cancel command must set a terminal status (it will release the place automatically).
6. **I2.2 (frontend, untouched here).** The Global Market accessibility findings and the raw-key /
   wage-unit issues from I2 §6 item 2 remain for the UI phase. The backend now returns the same
   encoded `squadFull` key on every route, so `resolveTranslatedErrorMessage` is still the right
   display path.

## 10. Verdict

**GO for the transfer-integrity scope**, subject to the validation companion's limits: the promised
place is now respected on every senior signing route, released by every status transition, covered
by boundary tests through production paths, and no save or schema migration is required. The
remaining limitations are pre-existing loan/academy capacity and a reused failure reason, both
named above; they do not invalidate the reservation rule.
