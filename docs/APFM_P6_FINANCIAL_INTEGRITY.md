# APFM P6: financial integrity

Branch `claude/p6-match-realism`, from `ea469f3a`. Local only: **not pushed, not merged**. Companions:
[`APFM_P6_MATCH_REALISM.md`](APFM_P6_MATCH_REALISM.md) and [`APFM_P6_VALIDATION.md`](APFM_P6_VALIDATION.md).
`external_registry.rs`, `external_world.rs`, `fm24-adapter/src/{registry,career}.rs`, `commands/registry.rs`,
`registryService.ts`, the locale files and the D7B3 worktree were not modified.

Evidence labels: **RUN**, **READ**. Each fix below was removed on its own in a disposable edit and its named test went red (nine
mutations, all killed), then restored byte for byte.

## 1. Summary

| # | Item of the brief | Finding | Status |
| --- | --- | --- | --- |
| 1 | Squad-floor wage-budget waiver can create persistent debt | The waiver covered **any wage**: a club short of one forward could sign the dearest one on the market whatever its budget said (P5's own test renewed a player from 12 000 to 250 000 a week on a 200 000 budget) | **Fixed**: bounded to 150 % of the club's own pay scale |
| 2 | No future wage cash-flow safeguard | A manager's, delegate's or registry's signing was judged against the *budget* only; nothing asked whether the bank can pay a wage for the length of the contract | **Fixed** on the human routes |
| 3 | Imported FM24 clubs enter debt without recruitment | Structural, in the imported data: 20 of 240 clubs start paying more than they can count on (up to 196 %). Not a rule of the game | **Fixed in the importer**, deliberately at 110 %, not 90 % (section 4) |
| 4 | Senior squad-capacity asymmetry | The limit of thirty held for a free agent, not for a contracted purchase (the managed club reached 34 in I1) | **Fixed for the transfer route**; the **external registry's contracted purchase is still exempt** (D7B3, section 7) |
| 5 | Pending commitments and delegated transactions | The manager's free-agent and renewal verdict ignored the wages of agreed deals; delegation's transfer *cash* ignored their fees | **Fixed** |

## 2. Commits

| Commit | Change |
| --- | --- |
| `814e02e9` | bound the squad-floor waiver, require cash to carry a wage, count agreed deals (checkpoint) |
| `fae3c613` | scope the cash rule to the human routes; tests state the bounded waiver |
| `ff28efe9` | purchases meet the senior-list limit; delegation's cash room reads agreed fees. **Correction:** its message says an external contracted purchase "now meets the limit too". **It does not**: `external_registry` does not call the changed path (section 7) |
| `04a3c559` | importer: recurring income carries day-one commitments up to 110 % of it |

## 3. The wage rules (`contract_wage_policy`, `ai_finance_guard`)

**Waiver (item 1).** The board's policy allows a bill up to 110 % of the budget. Over that, the squad floor used to waive it for *any*
contract while the club was short in the player's group. The waiver now covers a contract that **adds at most 150 % of the club's
median senior wage** (`FLOOR_WAIVER_SCALE_PCT`) to the weekly bill: for a renewal the raise, for a signing the whole wage. A club with
no wage to take a median of is not bounded. A club in debt may still fill its floor (`a_club_in_debt_may_still_fill_its_squad_floor`).
The check is lazy: it is asked only when the policy says no, so no sweep pays for it. AI sweeps that sign "the cheapest who will do"
(`BuyerWageFacts::new`) keep an unbounded ceiling: they cannot overpay the floor.

**Cash (item 2).** A contract that adds to the bill must be carried by the club's cash (`SigningBooks::cash_carries_wage`): not in
debt, and, when the bill would exceed the club's estimated income, with a season (`SAFETY_WEEKS` = 52) of that loss in hand: the AI's
own rule for a purchase, now shared. A contract that does not add (a renewal at the wage he is paid) is never refused for cash, and a
**floor signing is not asked**, so a club cannot be kept from fielding a side by its cash.
*Where it applies:* the manager-side verdict (`wage_policy_verdict`: renewals, free agents, delegated decisions) and
`BuyerWageFacts::of_committed` (a purchase being agreed now: bids, the registry). *Where it does not:* `wage_policy_verdict_at_bill`
(AI renewals), `joining_wage_policy_verdict` (youth), `BuyerWageFacts::of` (a registration), `::new` (AI sweeps).
**Why it is scoped:** the first version asked everywhere and broke four `squad_floor_seasons` tests (an AI club ended a season one
midfielder short): the AI's youth intake and academy promotions have never been held to cash, and the AI's free-agent signings are held
by the budget and a no-debt check by design (`ai_finance_guard`'s own note). The refusal reuses `be.error.contracts.boardWagePolicy`
(no locale file was touched), so **a cash refusal reads as the board's wage policy**: a limitation.

**Pending commitments (item 5).** `wage_policy_verdict` now counts `pending_wage_commitments` (agreed deals waiting to register), as
`of_committed` already did; a registration is judged by `purchase_wage_policy_verdict`, which does not read them, so no deal is counted
against itself. Test: `a_free_agent_is_judged_against_the_wages_of_agreed_deals` (fits before the deal, refused after).
**Delegation:** `books_of` subtracts `pending_fee_commitments` from the assistant's transfer cash (the wage room already did):
`the_assistants_transfer_cash_is_what_agreed_deals_leave` (5 000 000 in cash and budget, one agreed 2 000 000 deal: 3 000 000).
This closes P5's open item "delegation's cash room does not subtract pending fees".

**Capacity (item 4).** `execution::ensure_place_for`: a purchase meets the free agent's limit and refusal
(`be.error.freeAgents.squadFull?count=30&max=30&release=1`). It is asked **when a bid is made** (counting the places of agreed deals
waiting to register: `pending_incoming_purchases`) **and when a deal registers** (the live list). A loan that becomes a purchase needs no
place. Tests: `a_purchase_into_a_full_senior_list_is_refused_as_a_free_agent_is`, `an_agreed_purchase_holds_its_place_against_a_second_bid`,
`an_agreed_purchase_does_not_register_into_a_list_that_filled_in_the_meantime`.

**Two existing tests changed**, because they documented the unbounded waiver: `renewal_over_the_wage_policy_goes_through_when_the_squad_would_fall_below_the_floor`
now renews at a raise on the club's scale (12 000 → 17 000 on a 12 000 budget) and a new test refuses 250 000; the AI test
`a_club_goes_over_its_wage_policy_to_keep_a_player_it_needs_for_the_floor` is now
`a_club_does_not_pay_a_floor_player_beyond_its_pay_scale` (the AI also lifts a star to his value's wage, which is far beyond the scale of a
club whose only wage is his own; he is left to go and the floor is filled at what the club pays).

## 4. The imported FM24 world (item 3)

**Diagnosis (RUN, fresh import of the player's `world.db`, same seed):**

| | generated gate world | imported FM24 world |
| --- | --- | --- |
| Opening wages ÷ income a club can count on, median / max | 0.73-0.78 / 0.94 | 0.58 / **1.96** |
| Clubs over 1.0 on day one | 0 | **20 of 240** (17 of 68 English, 3 of 40 Italian) |
| Players on the books, max | 23 | 79 England, **101** Italy |
| Clubs in debt at season ends, no recruitment, original rules | 0 | **4, 8, 10, 4, 1, 0** of 239 (seasons 1-6) |

The clubs that go under are the lower English and Italian ones. The importer restored the source's wage budget, then **raised it to the
day-one commitments plus 5 %** (so no board sacks a manager in nine weeks), but the source's recurring income is the FM net, **floored
at zero**: nothing checked the income against the wages. The budget review then cuts a budget at most 5 % a year (15 % in debt) and never
cuts a wage, so a budget twice too high takes years to come down while the squads of 50 to 100 shrink by contract expiry (51 → 35 → 28 →
21 seniors). Debt peaks in season 3 and is gone by season 7: not persistent, but real, and with no signing at all.

**The rule.** The importer raises a club's recurring income, where it must, to the least that keeps its day-one commitments within a
cap of what it can count on (recurring income plus its own estimated gate); the raise is journalled, and recorded as OFM's (`ofm-générée`,
origin `adapter`) and never as the source's. Config `max_commitment_share_of_income_pct` (0 = off), CLI `--max-commitment-share=N`.

**The cap, tested rather than assumed.** A first cap of **90 %** was measured and **rejected**. Three worlds from the same `world.db`,
10 seasons, same seed, same rules (RUN):

| | cap off | **110 %** | 90 % |
| --- | --- | --- | --- |
| Clubs whose income is raised | 0 | **19** | 31 (12 of them never in debt without it) |
| Extra income, all clubs, a year | 0 | **113 M** | 235 M |
| Clubs whose source income was zero | | 7 | 11 |
| Largest raise a week (club 440) | | +470 k | +735 k |
| Opening wages ÷ income, median / max / over 1.0 | 0.58 / 1.96 / 20 | 0.58 / 1.32 / 16 | 0.57 / 1.08 / 6 |
| Clubs in debt, seasons 1-10 | 4, 9, 7, 3, 2, 1, 0, 0, 0, 0 | **0, 1, 1, 0 …** | 0 throughout |
| **The 19 clubs that need it:** debt club-seasons | 26 | **2** | 0 |
| their cash, median, season 1 → 10 | 3 → 24 M | **6 → 29 M** | 9 → **55 M** |
| their fees paid, ten seasons | 34 M | **29 M** | **180 M** |
| the 12 extra clubs of the 90 % rule: fees | 63 M | 80 M | 99 M |
| Whole world: cash P50 at season 10 / transfer budget P50 / fees, season 10 | 65.4 / 9.8 / 961 M | 64.6 / 9.7 / 1,012 M | 71.2 / 10.9 / 1,030 M |

At 90 % the clubs that needed help ended with **five times the transfer spending** and twice the cash they would otherwise have had, and
twelve clubs that were never in trouble were given income they did not need: that is inflation, not a fix. At **110 %**, the board's own
soft cap on a wage bill (`WAGE_SOFT_CAP_PCT`), the same 19 clubs follow the original trajectory (cash 6 → 29 M against 3 → 24 M, fees 29 M
against 34 M) with debt down from 26 club-seasons to 2, no healthy club is touched, and the world's cash, transfer budgets and fees are
within 10 % of the original. **110 % is the default.** The clubs are *not* forced into profit: at day one 16 still pay more than they earn
(wages ÷ income up to 1.32), one (`fm-c-444`) sits at 0-1 M cash in seasons 5-6 without going under, and 2 club-seasons stay in debt
(season 2-3). Those are the constraint kept credible.

**Over 30 seasons, default world (RUN):** 2 club-seasons in debt of 7,170 (seasons 2 and 3, one club), none in the 27 seasons after;
22 club-seasons over 110 % of the wage budget (11 in season 1, 3 in season 2, 2 in season 5, 1 in season 9, 2 in season 10, 1 in season 20, 2 in season 21, none otherwise);
senior squads 40.5 → 28.7 → 28.3 (season 1 → 5 → 10), stable at 28 to season 30; 784 MB, 840 s. Not in this fix and **not claimed fixed**: cash
at the rich end grows without bound (P90 118 M → 471 M → 1,770 M at seasons 1, 10, 30; these are unadjusted clubs, so it predates the
importer rule): the world has no sink for money at the top.

**What the user must do:** re-import the world (`apfm-import world.db <out>`): the SHA of the file changes, and clubs in an installed
world are not adjusted.

## 5. Tests

| Fix | Test (red without it) |
| --- | --- |
| Waiver bounded | `the_squad_floor_waives_the_policy_for_a_wage_on_the_clubs_scale_and_no_more`, `a_club_that_is_not_short_has_no_waiver`, `a_club_with_no_wages_has_no_pay_scale_to_bound_the_floor_by`, `the_squad_floor_does_not_waive_the_wage_policy_for_a_wage_far_beyond_the_clubs_scale` |
| Cash carries a wage | `a_wage_the_clubs_cash_cannot_carry_is_refused_though_the_budget_allows_it`, `a_club_in_debt_takes_on_no_new_wage_but_may_keep_a_player_at_his_own`, `a_club_in_debt_may_still_fill_its_squad_floor` |
| Pending wages (manager) | `a_free_agent_is_judged_against_the_wages_of_agreed_deals` |
| Delegation cash | `the_assistants_transfer_cash_is_what_agreed_deals_leave` |
| Capacity on purchases | the three `finance_integrity` capacity tests above |
| Importer | `a_club_that_cannot_carry_its_wages_is_given_the_least_income_that_can`, `a_club_that_can_carry_its_wages_gets_no_more_than_the_source_gives`, and the changed `the_source_s_net_non_wage_income_becomes_a_permanent_sponsorship` |

**Mutations** (remove only the fix, see red, restore): waiver ceiling (unit and renewal tests), cash rule, pending wages, the place check,
the pending-places count, the registration place check, delegation's fee subtraction, and the adapter's seating: **9 of 9 killed.**
The importer's rule was written together with its tests (not test-first) and **was not mutation-checked**: its tests failed once for fixture reasons (a club whose gate already carried its wages raised nothing), which is not the same evidence.

## 6. Limitations

1. A cash refusal on the manager's route reads as "the board's wage policy" (an existing key is reused; no locale was touched).
2. The cash rule uses a flat estimate of nineteen home matches for a club's gate and leaves out prize money, so it is slightly pessimistic.
3. The importer's raise is an owner's funding that world.db does not contain, recorded as OFM's; 7 of the 19 clubs get **all** of their recurring income this way.
4. Wealth at the top of the imported world grows without limit (section 4); not caused or fixed here.
5. 2 club-seasons of 7,170 still end in debt in the default imported world, 0 of 2,370 in the 30-season gate world.
6. The shipped (generated, 440-club) 30-season run is in the validation document.

## 7. For D7B3 (compatibility requirements)

No exported signature changed: `BuyerWageFacts::of_committed`, `transfers::fee_block_after_pending`, `pending_fee_commitments`,
`contracts::squad_capacity`, `ExternalOfferContext` and the registry's files are as in I1. **Behaviour D7B3 sees:**

1. `BuyerWageFacts::of_committed(...).purchase_verdict(...)` (the registry's wage check) now also asks that the club's **cash carry**
   the added wage, and bounds the squad-floor waiver at 150 % of the club's median senior wage. A registry signing that passed before can
   be refused when the club is in debt or has less than a season of a weekly loss in cash, or when a short club's floor signing asks more
   than 1.5 × its median wage. The refusal is the same `be.error.contracts.boardWagePolicy`.
2. **The registry's contracted purchase does not meet the senior-list limit.** `external_registry` checks `squad_capacity` only for a free
   agent and does not call the transfer path that now enforces it (the message of `ff28efe9` says otherwise: wrong). **D7B3 should apply
   `squad_capacity(game, club).blocks_signing` to a contracted external purchase too**, one line beside the free-agent check, with the same
   `squad_capacity(...).full_error()`; until then K4 stands for the registry route.
3. K1 (the registry's `order_clause` sorts `descending: true` ascending for ovr and value) is not P6's and is unchanged.
4. P6 changes **match-engine** behaviour, not `development` or `player_rating`: no change to the projection's inputs (K8 is unaffected).
5. `ExternalOfferContext`, `ExternalOfferDecision`, `ExternalRecruitPreview` and the valuation formula are untouched.
