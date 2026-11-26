# MARKET7 — AI squad management and transfer capacity

Written for: the owner and the independent (ChatGPT) source auditor of MARKET7.

**Verdict: see section 11.** Everything below says what was measured, what was changed, what was
only measured, and what was not done. Where a number comes from a run it names the run; the runs'
logs are in `~/apfm-scratch/market7/` (named in section 12).

## 0. Baseline and SHAs

* Required baseline `ca023ac0076fe216c7ae720e2ac715127f1b08d4` (confirmed: `git rev-parse HEAD` on
  `apfm/main`, clean tree).
* Final SHA: the last line of this file, written at push time.
* The original FM source database `~/apfootballmanager/data/world.db`: SHA-256
  `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` at the start; re-read at the end
  (section 10). Every source-backed run used `~/apfm-scratch/alpha6-world/apfm-fm24-2026.json`
  (SHA-256 `41b36c0b…2ea8`, read-only input); nothing was installed over a career, a world or a save.
* Not changed, as instructed: the 30-senior limit, the loan-return hold on a loaned-out senior's place,
  the valuation scale, the finance rules, the calendar. No calendar work was started.

## 1. The constraint, reproduced (step 1)

### 1.1 What was read

ALPHA6.1 §4 (AI market volume) and ALPHA6.2 (which carries the item forward unchanged), and the
implementation of `transfers/market.rs`, `registration.rs`, `interest.rs`, `buyer_view.rs`,
`ai_squad_sales.rs` (how AI clubs list players), `execution.rs`, `sale_view.rs`, `consent.rs`.

### 1.2 The baseline, reproduced on the same three seeds

`measure_ai_market_volume` (ignored, release, FM24 world of 310 clubs, seeds 1/2/3 × 3 seasons) built from
`ca023ac0` and the binary kept under a new name (`measure_volume_BASELINE_ca023ac0`). Between AI clubs only:

| Season | permanent transfers (seeds 1/2/3) | loans started | free-agent signings |
|---|---|---|---|
| 1 | **70 / 63 / 90** | 74 / 65 / 74 | 456 / 455 / 440 |
| 2 | 99 / 96 / 110 | 116 / 142 / 128 | 1196 / 1201 / 1180 |
| 3 | 123 / 112 / 133 | 129 / 127 / 155 | 1317 / 1362 / 1342 |

These are ALPHA6.1's "after the fix" figures (70 / 63 / 90); the funnel is the one it describes: on a typical
window day about 100 (season 1) to 130 (season 2) of the 310 clubs have no senior place, a club with no
place no longer shops (`clubs_without_a_senior_place`), and the hold on loaned-out places
(`registration::places_reserved_by`) is the reason a place is unavailable for most of them. Failed
purchases at execution: **0** (ALPHA6.1's fix removed the 3,000 a season). Registered senior counts: the
FM24 world *opens* with about 95 clubs over the limit (31–49 registered) and 307 clubs showing one
registered senior on the first window day of season 1; both are properties of the imported world and
are unchanged by this mission (section 9).

### 1.3 What the code did with a full club

A club with no place was **left out of the sweep entirely**. A club that wanted a right-back and had a
fourth keeper could only wait for another club to buy the keeper by chance. Nothing in the market could
make a place; places came only from contracts ending at the rollover.

## 2. What was implemented (step 2)

One new rule, built on existing authorities, in three small modules; nothing is a second transfer engine.

* `transfers/buyer_turn.rs` — **moved, not rewritten**. The per-target buyer judgement that sat as a ~300-line
  closure inside `evaluate_transfer_market` is now `BuyerTurn::would_sign` (`sporting_case`, `can_close`). It is
  the one rule for "would this club buy this player". Verified neutral: with room-making switched off
  the nine FM24 season totals, loans, free-agent signings and releases equal the `ca023ac0` baseline
  **exactly** (27 of 27 rows, section 5).
* `transfers/room_making.rs` — the new behaviour (`make_room`).
* `registration.rs` — `places_to_free_for_a_signing` and `senior_places_short`, the same arithmetic as the
  registration gate (`signing_place_refusal`, now built on the same `senior_places_held`), with a test that the
  two agree over eight list states, including a list already over the limit.
* `execution.rs` — `purchase_refusal_but_for_a_place`, a no-mutation rehearsal of the purchase.
* `sale_view.rs` — `AskingContext::fee_for`: the asking price construction that was inline in the
  shortlist, now shared by the shortlist and by a club selling a surplus player.

### 2.1 The flow

A full club shops only if it has a real need, on its own review weekday, and is short of **exactly one** place.
When it has chosen a signing by the ordinary buyer's case, and only then, `make_room`:

1. re-reads its place (a departure completed earlier in the sweep already frees it: **AlreadyFree**);
2. refuses if more than one place is missing (a loaned-out senior holds a place: one departure cannot make room);
3. rehearses the signing (`purchase_refusal_but_for_a_place`: floor of the seller, loan state, wage
   policy, terms, payable cash) and requires the signing to be worth its fee on the club's own scale
   (`is_worth_it`: not the waiver a club short of bodies gets);
4. picks candidates: seniors its squad model calls **Surplus** or **Unwanted** (`PlayerStatus`), outside the stored
   eleven, not on loan, not in a deal awaiting registration, not already moved today, not protected, and
   whose leaving keeps the squad floor;
5. **sells** the first candidate a taker wants (below), through `execute_transfer_with_terms`, as the first
   priority — only if the whole swap fits under the day's cap on completed deals (a swap spends two);
6. otherwise **reassigns** a senior no older than the academy limit (`YOUTH_ACADEMY_MAX_AGE`, the limit the
   manager's own command enforces) to the academy;
7. and only now carries out the signing through the same executor. If the signing fails after room was made it
   is recorded (`signing_failed_after_room`); it did not occur in any run.

The taker of a surplus player is found by asking up to six clubs (same country and nearest stature first)
the same `would_sign` question about him — need, price ceiling, budget, wage policy, books, the player's own
terms — with the price the selling club asks of a player it has put up for sale. The best fee wins.

### 2.2 What does and does not free a place

| Event | Frees a place? | Where it is enforced / tested |
|---|---|---|
| Completed permanent sale (any route) | yes | `a_completed_permanent_sale_frees_one_place_and_a_loan_does_not` |
| Departure completed earlier in the same sweep | yes (AlreadyFree) | `a_departure_already_completed_makes_the_room_…` |
| Reassignment to the academy (age ≤ 21) | yes | `a_surplus_youngster_with_no_taker_is_reassigned_to_the_academy` |
| Loan out | **no** (the hold is unchanged) | `a_loan_out_keeps_its_place_…`, `a_club_one_place_short_because_of_a_loan_…` |
| Transfer listing | **no** (nothing is sold) | listing is never read as a departure |
| Sale agreed, awaiting registration | **no**, and the player is not offered a second time | `a_pending_sale_does_not_free_a_place_and_is_not_sold_twice` |
| Contract ending before the signing date | not applicable | an AI club's signing registers the day it is made; contracts end at the rollover, which already releases them |

The "contract expiry before the signing date" case in the brief cannot occur on this path: the sweep runs only with the window open and
an AI signing registers that day. It is not implemented, because nothing could reach it.

## 3. Football awareness (step 3)

* The player who leaves is one the club's own squad model calls Surplus/Unwanted: the side the club
  fields would not miss him (`status_from_sale`: a sale costing under the model's `SURPLUS_LOSS`; a young
  player with room to grow is a prospect and never Surplus). A first choice, a rotation player and anyone in
  the stored eleven stay. The model is the one the sweep already priced sales with, so wages, age, potential
  and depth are inputs of it.
* The signing must be worth its fee for the side (`is_worth_it`), in addition to every test the club's ordinary
  purchase passes. In the nine ON seasons, of the completed swaps the signing would **start** for the club in its
  own model in 35/57, 21/29, 21/24 (seed 1), 24/51, 20/34, 21/36 (seed 2), 27/57, 26/38, 22/31 (seed 3): between
  47 % and 88 % of a run's swaps; the rest are cover that the model still values above its price. Mean rating points added to
  the side: 4.5–5.4 per swap.
* The manager's club takes **no part**: it is never a retained buyer when full, is never offered to by a
  full club (to buy *or* to borrow — a bug the first version had and an existing test caught), is never a taker
  and never sells or reassigns on its own. Delegation protection (`delegation.is_protected`) is honoured by
  the candidate filter as well, though it can only name the manager's players.

## 4. Transaction integrity (step 4)

* Every departure is a call to `execute_transfer_with_terms` (the authority every AI-to-AI purchase uses) or the
  same academy-registration rule the manager's command uses; every signing is the same call. The executor
  validates before it mutates: a refusal leaves both clubs, the player and the journal untouched
  (`a_transfer_refused_for_want_of_a_place_…`, `…_break_the_squad_floor_…`, with the refusal reason asserted).
* The sale completes **before** the signing; the signing is rehearsed before the sale; a sale that the
  transfer refuses abandons the attempt (`sale_refused_by_transfer`: 0 in all runs).
* After every completed deal both clubs' wage bills are recounted and the set of clubs with no place is
  refreshed, so a club that took a surplus player and is now full cannot take another. A pending sale is
  filtered out, so a negotiation in progress is never duplicated (test, and a save/reload test).
* Two clubs after one player: exactly one signs him, and the other sells nobody for a signing it did not get
  (`two_full_clubs_after_the_same_player_make_one_swap_not_two`).
* Measured across the nine seasons of each run: **0** clubs whose cash differs from their journal, transfer-fee
  posts net across the world **0**, **0** duplicate same-day permanent transfers, **0** executions refused,
  **0** invariant violations (the harness's `Invariants` suite).
* Not changed: the 30-senior limit, the hold, valuations, finance. The cap on the day's completed deals still
  applies (a swap fits under it whole).

## 5. Results (step 5)

Runs: FM24 world (310 clubs), seeds 1/2/3 × 3 seasons, release; and the shipped generated world (440
clubs), same seeds. "Baseline" is the `ca023ac0` binary; "OFF" is the new code with room-making switched off
by `APFM_ROOM_MAKING=0` in the same binary as "ON".

### 5.1 FM24 world, between AI clubs only

| Season | baseline = OFF permanent | **ON permanent** | loans ON (OFF) | free-agent ON (OFF) |
|---|---|---|---|---|
| 1 | 70 / 63 / 90 | **152 / 133 / 150** | 77 / 80 / 78 (74 / 65 / 74) | 454 / 457 / 458 (456 / 455 / 440) |
| 2 | 99 / 96 / 110 | **142 / 148 / 156** | 142 / 146 / 137 (116 / 142 / 128) | 1211 / 1195 / 1204 (1196 / 1201 / 1180) |
| 3 | 123 / 112 / 133 | **162 / 162 / 164** | 170 / 148 / 147 (129 / 127 / 155) | 1316 / 1321 / 1321 (1317 / 1362 / 1342) |

OFF equals the baseline in every one of the 27 cells for permanent transfers, loans, free-agent signings and
releases (a check printed by the comparison script). So the refactor changed nothing, and everything in the
ON column is the new rule.

### 5.2 Why the volume rises, and whether the extra is supported

First season: ON − OFF = 82 / 70 / 60. The room-making deals (completed swaps + the sales that made them) are
57 + 25 = 82, 51 + 22 = 73 and 57 + 22 = 79: they account for the rise, less some drift (the two worlds
diverge once the first swap happens, so the other deals are not identical). It is not a loosening of any buyer's test: every
extra signing passed the club's ordinary case *and* `is_worth_it`, and every extra departure was a player its
model called surplus. Evidence from the logged swaps (every swap is recorded with both players):

| | season 1 | season 2 | season 3 |
|---|---|---|---|
| completed swaps (3 seeds) | 57 / 51 / 57 | 29 / 34 / 38 | 24 / 36 / 31 |
| …by permanent sale | 25 / 22 / 22 | 3 / 3 / 0 | 1 / 2 / 3 |
| …by academy reassignment | 32 / 29 / 35 | 26 / 31 / 38 | 23 / 34 / 28 |
| departed player's status | 52–55 surplus, 2–5 unwanted | all surplus | 24–36 surplus (0–2 unwanted) |
| mean age departed → signed | 23.0–23.4 → 23.9–24.9 | 20.6–21.2 → 24.5–26.8 | 20.7–21.5 → 25.7–27.5 |
| mean rating departed → signed | 69.7–70.5 → 69.9–71.7 | 65.7–67.4 → 71.2–73.0 | 66.2–66.9 → 73.0–74.3 |
| mean weekly wage departed → signed | 13.5–17.1k → 12.7–18.1k | 4.4–5.1k → 7.0–11.5k | 1.5–2.6k → 6.5–16.7k |
| sold players: fee ÷ value | 0.83 / 0.84 / 0.84 | 0.85 / 0.84 / – | 0.77 / 0.81 / 0.75 |

Reading, honestly:

* The first season is mostly clubs that had a full list of surplus cover and a hole in the eleven; under half of
  the swaps (25 of 57, 22 of 51, 22 of 57) are sales to a club that wanted the player, at a mean 0.83 of his value.
* **From the second season on, about nine in ten swaps are academy reassignments** (a 21-year-old surplus
  player of rating ~66 goes to the academy; a 25–27-year-old of ~73 is signed). Sales almost disappear because
  there is nobody with a place to take them (the same hold keeps 120–135 clubs full a day). This is the rule
  working as specified (a reassignment is an explicitly permitted departure), but it is a **policy
  consequence the owner should look at** (section 10): the market's own volume rises by 20–50 % while the
  quality of the squads barely moves (below), and the age profile of signings shifts older.
* Of the reassigned players, 1–9 per run (up to 26 %) are seniors again before the rollover (they pass the
  academy's age limit and graduate through the existing, place-gated path): a reassignment is not reversed by
  force, and it did not breach any limit.

### 5.3 Quality, wages and cash

* Mean rating of every AI club's best 18 seniors (window open → before rollover), OFF vs ON, season 1: 72.425 →
  73.025 vs 73.034 (seed 1); seasons 2 and 3 differ by −0.02 to +0.08. **The extra transfers do not make the
  squads better on average; they place better-fitting players where the sides had holes.** Each swap adds
  4.5–5.4 rating points to the buying club's modelled side.
* Wage and cash effects of the swaps: season 1 the buyer's mean new wage is within ±4k of the departed one;
  later seasons the signed player earns more than the reassigned youngster (the reassigned costs the club
  nothing and is not paid off). Cash: fees paid exceed fees received in every season (e.g. seed 1 season 1:
  218.4 M paid against 144.3 M received by the swapping clubs). World-level: AI clubs in debt ON/OFF
  (season 1/2/3): 11/15/18 vs 11/17/16 (seed 1), 9/15/15 vs 9/18/19 (seed 2), 11/16/16 vs 12/16/19 (seed 3);
  mean AI-club cash within 0.4 % of OFF except seed 2 (+2.1 % and +1.6 % in seasons 2 and 3). No club is pushed into debt by it.

### 5.4 The funnel and the capacity

* Clubs with a full list per window day (mean), OFF vs ON: season 1 ≈ 100 → 98; season 2 130–135 → 122–129;
  season 3 78–82 → 78–79. **Room-making does not relieve the capacity problem**; it lets about 30–60 clubs a
  season use the places they would anyway have found.
* Clubs that found a target but lacked a place (distinct clubs): 62 / 66 / 62 (season 1), 29 / 32 / 39 (season 2),
  36 / 41 / 36 (season 3). Clubs that attempted to free space: the same (every such club attempts); clubs that freed
  a place: 38 / 35 / 36, 22 / 26 / 31, 19 / 26 / 24; clubs that did not: 55 / 57 / 53, 11 / 15 / 19, 22 / 23 / 18
  (a club can appear in both across the season).
* Refusals (seed 1, season 1, 361 attempts): no taker 268, signing not worth a departure 32, no player to let go
  4, daily cap 4, sold 25, reassigned 32, **too many places short 0** (the market-level filter removes those clubs
  before they shop), signing would fail 0, sale refused by the transfer 0.

### 5.5 Registration violations, failed registrations, duplicates

| | OFF | ON |
|---|---|---|
| club-days over the 30-senior limit (season 1 / 2 / 3, seed 1) | 4830 / 4944 / 2287 | 4800 / 4751 / 2262 |
| of which new crossings from ≤ 30 to > 30 | 2 / 42 / 2 | 1 / 44 / 0 |
| new floor breaches (15 seniors, 2/4/4/2) | 544 / 9 / 3 | 544 / 7 / 4 |
| executions refused (`buyerSquadFull` etc.) | 0 | 0 |
| duplicate same-day permanent transfers | 0 | 0 |

No violation is introduced: the over-limit club-days are the imported world's (section 9), and the number of
new crossings is the same with and without the new code (the ~40 in season 2 appear in OFF as well: 42 / 40 / 43).
For the first season I printed the cause of every crossing in the ON run (four, seed 1): each was a player
returning from loan on a day with no market move at the club (ledger entry `LoanReturn`) who became a senior on
return with no place check, which looks like the loan-return path and not the market; the ~40 season-2 crossings
were not traced one by one. Reported as a pre-existing defect in section 9 and not touched (the brief forbids
changing loan-return reservations).

### 5.6 Generated world

440 clubs, seeds 1/2/3 × 3 seasons: permanent transfers 62 / 25 / 203, 36 / 41 / 222, 34 / 24 / 218 with room-making on
**and** off — identical. No club of a generated world ever reaches 30 seniors in these runs (club-days over the limit
0, full clubs 0), so the senior limit is not the constraint there and the new rule never has a case. No
regression, 0 invariant violations, sweep time unchanged.

## 6. Performance (step 6)

* `market.rs` production code (up to the first `#[cfg(test)]`): 1,495 → **1,293 lines** (limit 1,500).
  `evaluate_transfer_market`: 985 → **766 lines**; still over the 100-line function limit and the
  cognitive-complexity limit (the architecture reviewer's P1, section 8). It is debt that predates the mission
  and was reduced, not fixed; splitting the rest of the shortlist and loan loops is a separate refactor named for
  epic #589.
* The AI market sweep (`SWEEP_NANOS`, the time inside `evaluate_transfer_market`), FM24 world, summed over
  the three seeds and nine seasons: **64.3 s OFF → 75.1 s ON (+17 %)**. By season: 4.3 → 5.5, 8.2 → 9.4 (first
  seed); the extra is the buyer turns of the full clubs that now shop (≈ 735 more in a first season: 1,160 →
  1,895) and the taker search. Whole-run wall time: 400 s → 413 s (+3 %). Generated world: no difference
  (an earlier ON/OFF pair of runs of the same code paths gave 20.1 s / 20.1 s, 24.7 s / 25.9 s, 24.3 s / 24.4 s of sweep for seed 1).
* What bounds it: only clubs short of exactly one place, with a real need, on their review weekday, shop
  while full; at most three surplus players with a possible taker are offered per attempt and at most six
  clubs weighed per player; the place sets are refreshed with one world scan for each of the two clubs of a
  completed deal. The log of swaps is capped (5,000 entries) so a long career cannot grow it without bound.

## 7. Tests (step 7)

### 7.1 New and changed

`transfers/room_making_tests.rs` — 19 scenario tests, each with a Given/When/Then, run through the real
`evaluate_transfer_market` on a hand-built world (a full club, a seller, a taker, the manager's club):

1. `a_full_club_sells_its_surplus_player_and_then_signs_the_one_it_wanted`
2. `a_full_club_with_no_acceptable_departure_makes_no_signing`
3. `a_protected_player_is_never_the_one_who_makes_room`
4. `the_club_does_not_sell_a_first_choice_to_buy_a_better_one`
5. `a_player_in_the_stored_eleven_is_not_the_one_who_makes_room`
6. `a_loan_out_keeps_its_place_and_does_not_make_room`
7. `a_club_one_place_short_because_of_a_loan_makes_that_one_place_by_a_sale`
8. `a_completed_permanent_sale_frees_one_place_and_a_loan_does_not`
9. `a_pending_sale_does_not_free_a_place_and_is_not_sold_twice`
10. `a_replacement_the_club_cannot_afford_makes_no_departure` (with a funded control)
11. `a_transfer_refused_for_want_of_a_place_leaves_both_clubs_unchanged`
12. `a_transfer_that_would_break_the_squad_floor_leaves_both_clubs_unchanged`
13. `two_full_clubs_after_the_same_player_make_one_swap_not_two`
14. `the_managers_club_takes_no_part_in_making_room`
15. `a_negotiation_in_progress_survives_save_and_reload` (a swap decided identically from the saved game)
16. `a_departure_already_completed_makes_the_room_and_nobody_else_is_sold`
17. `a_surplus_youngster_with_no_taker_is_reassigned_to_the_academy`
18. `the_pre_flight_refuses_exactly_what_the_transfer_refuses`
19. `the_pre_flight_leaves_only_the_place_to_the_transfer`

`transfers/tests.rs`: the one-pass place count now also asserts the number of places missing, agreeing with
the gate. No existing test's threshold or assertion was weakened; the existing
`a_club_whose_senior_list_is_full_does_not_shop` caught a defect of the first version (a full club offering for the
manager's player) and is unchanged.

### 7.2 Red first, and mutation

The behaviour was written before its tests (a deviation from "tests first", stated): the red evidence is therefore
mutation, not an original red run. Disposable in-place edits, each restored and re-touched, each run through
`cargo test -p ofm_core --lib transfers::`:

| Mutation | Result |
|---|---|
| no departure candidates | 2 tests red (sell-and-sign, academy reassignment) |
| protected players may be sold | protected test red |
| pending registrations may be sold again | 2 red (pending, reload) |
| any age may be reassigned | 4 red |
| manager's club may take a surplus player | manager test red (the first fixture did not discriminate; strengthened) |
| a full club may offer for the manager's player | existing `a_club_whose_senior_list_is_full_does_not_shop` red |
| stored-eleven guard removed | stored-eleven test red |
| status filter loosened (any player may go) | sell-and-sign red |
| `short > 1` guard in `make_room` **and** the market-level `== Some(&1)` filter removed | loan-out test red (each alone survives: they are two layers of one invariant) |

Survivors, named: the daily-cap check for a swap (`completed + 2 <= max`) and the signing's
`is_worth_it` gate have no test that fails when they are removed; the sweep-level paths `sale_refused_by_transfer` and
`signing_failed_after_room` are never reached by a test (they were 0 in every run). The test reviewer's list
of weaker spots was addressed (exactly-one assertion, funded control, reasons asserted, one-short loan sweep,
swap after reload, a full-buyer parity case); the save test is a JSON round trip, not the `db` writer with a
fresh reader (no persisted field was added).

### 7.3 Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean |
| `cargo test --locked --workspace` default / `--features mcp` | see 7.4 |
| frontend `npm test` | 3,580 passed, 4 skipped (399 files) |
| `npm run build`, `npm run lint`, `npm run format:check`, `npm run knip` | pass (3 pre-existing lint infos, knip hints only) |
| i18n gate `vitest run src/i18n` | 37 passed |
| source-backed harness runs with the `Invariants` suite (FM24 ON ×3 seeds, FM24 OFF ×3, generated ×3) | 0 violations in all nine |

No player-facing string was added (the room-making has no message, news item or error that a player reads), so no
locale file changed and `i18n-auditor` had no surface to review. No frontend file changed.

### 7.4 Final run of the suites (on the code that is pushed)

* `cargo test --locked --workspace`: **5,752 passed, 0 failed**, 160 ignored (the harness studies). `--features mcp`:
  **5,839 passed, 0 failed**, 160 ignored. (An earlier run on the same code failed `only_world_generation_stages_a_contract`
  because the new test file was read as production code; fixed, section 8.)
* Source-backed tests with `APFM_WORLD_JSON` set to the scratch FM24 world (read-only), release:
  `fm24_global_market` 3 passed (a market in motion survives a save and a reload; clubs outside the player's league buy;
  wage bills stay near budgets) with `the_market_reaches_its_historical_targets` still `#[ignore]`d for the ALPHA6.1
  owner decision, **no threshold was changed**; `fm24_career` 2, `fm24_loans` 4, `fm24_free_agents` 4,
  `delegated_recruitment` 2 passed.
* `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` clean; clippy default and `mcp` clean at
  `-D warnings`; frontend gates in 7.3.

## 8. Reviewers

Architecture, dedup and test reviewers ran on the diff (the transfer-realism and persistence reviewers are not
available as agent types here; the persistence question is answered by "no serialized field was added",
confirmed by the architecture reviewer: none of the new types derives `Serialize`, no `domain`/`db` file changed).

| Finding | Severity | Disposition |
|---|---|---|
| `evaluate_transfer_market` still 766 lines / cognitive > 25 | P1 (rule) | reduced from 985, the room-making block hoisted (`RoomFacts` built once); **remains** — named for epic #589 |
| swap log unbounded in a long process | P2 | fixed (capped at 5,000) |
| `ENABLED` gated admission only | P2 | fixed (`making_room &&` at the call) |
| reassignment did not invalidate the sale view | P2 | fixed |
| visibility wider than needed | P2 | fixed (`pub(super)`) |
| `purchase_refusal_but_for_a_place` hand-copies the executor's order (drift risk) | P2 / dedup D2 | **parity tests added** (19 and 18 above); a shared validator is not done |
| academy reassignment duplicates the manager command's rule; the two differ (`remove_player_references` vs XI only) | dedup D1 | **not consolidated** (would change `commands/squad.rs`); the manager's path leaves a stale captain/penalty-taker reference when he is sent to the academy — a **remaining defect**, section 9 |
| harness re-states the floor | dedup D3 | fixed (uses `MIN_SENIOR_PLAYERS`, `MIN_PLAYERS_PER_GROUP`); the journal sum is kept as a one-pass loop for speed |
| `room_making` picks sellable players from the sweep's `statuses`, `ranked_sellable` is another ordering | dedup J4 | documented, epic #589 |
| test reviewer: weak assertions in competing-clubs, unaffordable, failed-transfer, loan, reload | P1 | all six fixed (7.2) |

A fifth item found by the suites: `tests/contract_single_writer.rs` scans non-`tests.rs` files for the contract-staging
helpers; the new test file declares `#![cfg(test)]` so it is read as test code.

## 9. Remaining defects and open decisions

1. **Pre-existing: the imported FM24 world opens over the limit** (≈ 95 clubs with 31–49 seniors; 307 clubs
   with one registered senior on the first window day). Not touched (the world is read-only).
2. **Pre-existing, probable: a player returning from loan becomes a senior with no place check** (seen in all
   four first-season crossings traced; about 40 clubs cross 30 in season 2, with or without room-making, not
   traced individually). The loan-return reservation
   is not to be changed by this mission; the owner should decide (hold the place for over-age academy loanees,
   or shed on return).
3. **Policy for the owner:** from the second season about nine in ten swaps are academy reassignments of ~21-year-old
   surplus players; the clubs' squads do not improve on average. Options: cap reassignments per club per season;
   require the reassigned player to be below an ability line; or leave as is. The system is correct as specified;
   whether the effect is wanted is a design call.
4. The room-making does not touch the hold on loaned-out places, so 120–135 clubs a day are still full in season 2;
   the AI market remains well under the 352–383 a season ALPHA6.1 measured with no hold. Not a defect.
5. The manager's command for sending a senior to the academy leaves a stale captain / set-piece role (dedup D1).
6. `evaluate_transfer_market` is still over the function-size and complexity limits (section 6).
7. Sweep-level paths `sale_refused_by_transfer` and `signing_failed_after_room` are untested (never reached); the
   daily-cap check for a swap and the `is_worth_it` gate have no discriminating test.
8. GUI not run: no UI file changed.

## 10. Safety

* The original FM source database was only read. SHA-256 at the start and at the end:
  `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` (re-checked at push time, below).
* No installed career, installed world or personal save was opened or modified; the harness builds worlds in
  memory from the read-only scratch JSON or the generator.
* The `~/apfm-scratch/market7/` directory holds only logs and binaries.

## 11. Verdict

**CONDITIONAL GO for the independent audit.** What is established: the new rule is behaviour-neutral when off
(27/27 cells equal the baseline), makes room only by a completed sale or a permitted reassignment, never by a loan
or a listing or a pending sale, never touches the manager's club, keeps every transaction on the existing
executor, introduces no violation, no duplicate, no cash mismatch and no failed execution in the nine FM24 seasons
(three seeds) it was run on and the nine generated-world seasons, and costs +17 % of the AI market sweep. What is not established, and
is the owner's call: whether a 20–100 % rise in permanent transfers, nine-tenths of it academy reassignments from
the second season, is the market the game should have (section 9, item 3), and the three pre-existing defects
(items 1, 2, 5).

## 12. How to reproduce

* Baseline binary: `~/apfm-scratch/market7/measure_volume_BASELINE_ca023ac0`; final binary
  `measure_volume_FINAL2`; both run with `--ignored --nocapture` and `APFM_WORLD_JSON`, `APFM_SEEDS=1,2,3`,
  `APFM_SEASONS=3`; `APFM_ROOM_MAKING=0` plays the market without room-making; `APFM_GENERATED=1` the generated world.
* Logs: `baseline-volume.log`, `final2-fm24-rm1.log` (ON), `final2-fm24-rm0.log` (OFF), `final2-generated-rm1.log`.
* In the repository: `cargo test --release -p season-harness --test measure_ai_market_volume -- --ignored --nocapture`.
