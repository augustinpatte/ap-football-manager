# Run C — realistic free-agent negotiations

Branch `claude/gameplay-transfer-realism`, based on `validation/runs1-4-m12`.
Scope: how a player without a club is signed. Runs A (delegation), B (time advancement) and D (player
database / M12 viewer) are not touched; the interfaces they need are listed in §5.

## 1. The defect, and where it was

A Brussels FC-type manager signed free agents rated ~82 on the spot. The cause was not a bypassed check.
Every route went through the same code, and that code never asked the questions the mission lists.

`offer_free_agent_contract_with_terms` (ofm_core `contracts/free_agent.rs`, before this run) signed the
player in the same call as soon as `wage ≥ expected_wage && years ≥ expected_years`. The only inputs were the
player's *own* history (his last wage, age, morale, market value) and the board's wage policy. Nothing read the
club's reputation, its squad level, the place he would have in the side, other clubs, or how long he had been
out of work. An 82 with a 100 000 a week history accepted a 300-reputation club that offered 130 000.

### Acquisition routes audited

| Route | Entry points | Finding |
|---|---|---|
| Free agent, manager | Modal opened from Transfers, Player profile, Scouting and Players list → `useFreeAgentContractFlow` → Tauri `offer_free_agent_contract` → `offer_free_agent_contract_internal`. MCP `transfer_free_agent_offer` calls the same `_internal`. | One backend path, no frontend or MCP bypass. **Defect:** instant signing, no sporting or competition test. Fixed. |
| Free agent, AI floor / planning | `squad_floor::bring_in_one` → `sign_free_agent_for`; `ai_contracts::improve_squad` | Instant, best-rated affordable player, whatever he would think of the club. **Fixed** for clubs that are not at the floor (§3.5). A club that *must* fill its floor still takes who it can, as before. |
| Completing any free-agent signing | `contracts::sign_free_agent` (`pub(crate)`) | Did not check that the player was still without a club, so two routes could both "sign" him. **Fixed**: refuses a player with a club; closes every other open talk. |
| Paid transfer, manager | `transfers::make_transfer_bid_with_terms` | Clubs agree on the fee, funds and wage policy are checked, the player has a veto only if he is young and ambitious and the club is 100+ reputation below his (`player_refuses_move`). **Not changed** (see §6). |
| Paid transfer, AI | `transfers::evaluate_transfer_market` | Already gated: a buyer is not considered if the seller out-reputes it by more than 150 (`MAX_BUYER_REPUTATION_DEFICIT`), plus the wage, budget and need tests of the mission-10 market. Not changed. |
| Loans | `transfers::loans` | Owner must list or accept; borrower wage policy checked. Not changed. |
| Academy promotion | `roster`, `squad_floor` | Not an acquisition from outside the club. Not changed. |

## 2. What a negotiation is now

An offer opens a **talk** on the player (`domain::free_agent_talk::FreeAgentTalk`, stored in the JSON column that
already holds his morale record, so no SQL migration and old saves read it as empty).

```
offer ──► AwaitingResponse ──(1–3 game days)──► Signed
   ▲            │                              ├─► CounterOffered ──(manager accepts)──► Signed
   │            │                              │        │ (manager re-offers: round + 1, max 4)
   │            ├─ manager withdraws ► Withdrawn         ├─ unanswered 3 days ► Lapsed
   │            ├─ rival signs him ► LostToRival         └─ round 4 not met ► Rejected (talks broken off)
   │            └─ club can no longer pay ► Lapsed       
   └─ insulting offer (< 65 % of his worth): Rejected at once, club not heard for 14 days
```

* The answer day is 1–3 days after the offer, derived from a stable hash of player, club, date and round, so a
  reloaded save is answered on the same day. The reply is visible as `respond_on`.
* A second click while a talk is awaiting an answer is refused (`be.error.freeAgents.talkAlreadyOpen`); the
  frontend also disables the button and says "Offer pending".
* At most 5 talks open for the manager at once; at most 30 seniors registered at the club.
* Money is checked twice with the same rules: when the offer is made and when the player says yes (wage
  policy, signing bonus against cash and transfer budget). A club whose books changed in between loses the deal
  (`Lapsed`, reason `club_cannot_afford`) and nothing is signed.
* The senior-list limit is checked at the same two moments, with places promised to agreed purchases set aside. A talk
  whose place disappeared in between lapses with reason `no_senior_place` (a capacity problem, not a money one).

### How the player decides (`contracts/free_agent_talks/appeal.rs`)

One score; at ≥ 0 he signs, from −18 to 0 he asks for other terms, below that he refuses.

| Term | Range | Reads |
|---|---|---|
| wage | −25 … +20 | pay + bonus spread over the contract, against what he expects (+2 points per 4 % over) |
| sporting | −∞ … +12 | his standing (rating, potential if young) against the club's (60 % reputation, 40 % level of its best 14 seniors). Free up to 60 points of standing; beyond that, points per 8 standing × his ambition (young and gifted ×1.3, veteran ×0.55) |
| role | −12 … +12 | key player / starter / rotation / backup among the club's seniors of his position group |
| duration | −6 per year short; −4 per year over 2 for 33+ | |
| situation | 0 … +20 | no effect for 21 days out of work, then a point every 3 days |
| promise | +8 | stepping-stone promise, only if he values it |

An elite free agent therefore signs for a much smaller club only by combining pay near the top of the scale
(+40 %), the certainty of being its best player, and a long wait without an offer.

### Rival clubs (`rivals.rs`)

Each day the 25 best free agents (rating ≥ 62) are offered to a rotating sample of 12 clubs. A club bids if he
would improve its side, would accept *its* offer by the same score, and it can pay him (wage policy and
books); at most 6 new bids a day, 3 open per player. Their bids are ordinary talks (`made_by_manager = false`).
When a player's answer falls due he weighs every offer on his desk and signs the best one that clears zero; the
others are `LostToRival` and the manager is told who got him.

The AI's instant squad-planning signings now skip a player who would decline the club on standing alone
(`declines_on_standing`), unless the club needs him to fill its floor.

### Free agents and the transfer window

Not bound to it, as before (existing tests `free_agent_can_be_signed_when_transfer_window_is_closed` and
`..._on_deadline_day` still hold). The deadline sweep only touches paid negotiations; a free-agent talk open on
deadline day resolves normally. This is deliberate (unattached players may be registered at any time in England
and in the previous design) and is the thing to change if a stricter league rule is wanted.

## 3. Before / after, concrete

Numbers are from `tests/free_agent_talks_tests.rs`; the fixture is a club of reputation 300 whose seniors are
rated 60, and an 82-rated 28-year-old asking 130 000 a week over 3 years.

| # | Scenario | Before | After |
|---|---|---|---|
| 1 | Modest club offers his full ask | Signed immediately | Preview says *unlikely* (main obstacle: club below his level). He answers in 1–3 days, refuses, no contract, inbox message |
| 1b | Same club, he has been unattached 80 days, offer +40 %, he would be its best player | Signed immediately | Signs (score −47 + 20 + 12 + 19 = +4). The same raise on a fresh free agent, or his bare ask after 80 days, is still refused |
| 2 | Wage the board cannot carry / bonus beyond cash | Wage: refused. Bonus: refused | Same refusals, now before a talk exists; and re-checked on the day he says yes (`Lapsed` if the club's books moved) |
| 3 | 58-rated player, club level 60, his ask | Signed immediately | Pending, signs on day 1–3, one contract, one movement record |
| 4 | 70 % of his ask for one year | Counter / reject at once | Pending, refuses on his day with reason `wage_too_low`; nothing signed |
| 5 | 85 % of his ask | Counter at once | Counter on his day (inbox + panel); *accept* signs on his terms; accepting twice is refused; ignored 3 days → lapsed |
| 6 | A much bigger club needs him | Never competed | It bids by day 1; if its offer is due first/better he signs there, the manager's talk is `LostToRival`, one contract only |
| 7 | Save/reload mid-talk | n/a | `serde_json` round-trip mid-talk, then 6 more days: identical talks, owner and inbox as the unsaved game; SQLite round-trip of all three talk states; a save without the field loads with none |
| 8 | Deadline day | Signable | Deadline sweep leaves the talk alone; he still answers and signs |
| 9 | Click twice / two clubs / counter accepted twice | Possible duplicate attempts | `talkAlreadyOpen`, `playerNotFreeAgent`, `noCounterToAnswer`; one `FreeAgentSigning` record |
| 10 | Paid transfer in instalments, then a free-agent signing | n/a | Obligations and journal unchanged by the signing; next instalment paid once when due |

## 4. What changed

* `domain`: `free_agent_talk.rs` (new); `PlayerMoraleCore.free_agent_talks` (`serde(default)`).
* `ofm_core/contracts`: `free_agent_talks/{appeal,workflow,rivals,consts}.rs` (new). `free_agent.rs` loses the
  instant `offer_free_agent_contract*` functions; `sign_free_agent` becomes the guarded single path.
* `ofm_core/squad_floor.rs`: AI instant signings respect `declines_on_standing`.
* `ofm_core/turn/mod.rs`: one call, `process_free_agent_talks`, in `process_day_common`.
* Tauri: `offer_free_agent_contract` (now returns pending), `accept_free_agent_counter`,
  `withdraw_free_agent_offer`, `preview_free_agent_contract_impact` (+ interest, rivals, open talk).
* MCP: `transfer_free_agent_offer` answers `pending`/`rejected`; `transfer_free_agent_preview` reports interest,
  the open talk, his counter and rival clubs, takes an optional `contract_years`. To take a counter, the agent
  offers his asked terms (a new offer on a counter is a new round). No new tools.
* Frontend: `FreeAgentTalkPanel` inside the existing contract form; the hook tracks the talk from the game
  state; `be.error.freeAgents.*`, `be.msg.freeAgentTalk.*`, `freeAgents.*` in all 12 locales.

## 5. Interfaces for the other runs

* **Time advancement (Run B):** call `ofm_core::contracts::process_free_agent_talks(&mut Game)` once per game
  day. It is already in `process_day_common`, which both ways of ending a day share, so a multi-day advance
  needs nothing more. `free_agent_talks_awaiting_manager(&Game) -> usize` counts counters the manager has not
  answered, if advancement wants to stop for them (not wired: `time_blockers` is Run B's).
* **Delegation (Run A):** `submit_free_agent_offer`, `accept_free_agent_counter`, `withdraw_free_agent_offer`
  take `&mut Game` and act for the manager's club under the same validation; `assess_free_agent_interest` is the
  read-only "would he?" an assistant can consult before offering.

## 6. Remaining limitations

* **Paid transfers and loans are not given the player-willingness score.** They keep their own rules (the fee
  negotiation, the stepping-stone veto, the AI reputation-deficit gate), which I judged not safe to retune without
  breaking the 130 existing market tests and the mission-10 market calibration. A manager with the money can
  still buy a star from a bigger club. Reusing `appeal` for `player_refuses_move` is the obvious next step.
* Free agents are not bound to the transfer window; there is no foreign-player quota, only a 30-senior cap.
* The reputation scale (0–1000) and ratings were tuned by reasoning and unit tests, not by a multi-season
  simulation: no measurement of how often AI clubs now win free agents over a season was made (the harness
  `apfm_economy` was not run).
* Response delays are deterministic per (player, club, date, round), not random; two clubs bidding on the same
  day get different answer days but no further variance.
* The wage-policy waiver notice the old modal could show ("the board waived its budget to keep the squad floor")
  is gone with the old feedback panel.
* `mcp-e2e` and the real Tauri window were not run. The M12 engine is absent from this sandbox (a stub outside
  the repository was used, as in earlier runs); six M12 tests fail by construction and nothing here validates M12.
* `agentic-fc` (MIT) was read for ideas. Its public design notes only list transfer-acceptance base rates and
  describe AI clubs as running the same decision machinery as the player; they contain no recruitment policy to
  reuse. Nothing was copied or adapted, so no attribution is carried.
