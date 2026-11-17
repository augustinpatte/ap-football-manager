# APFM I4A: strategic club AI

Branch `deepseek/i4-strategic-ai`, based on `apfm/main` at `0dc3582dc8a4c0b7b7a87d6746377c7618f90d36`.
Evidence, commands and limits: [`APFM_I4A_VALIDATION.md`](APFM_I4A_VALIDATION.md).

## 1. What was wrong

An AI club answered three separate questions from three separate readings of its own squad:

* the transfer market asked which **starting slots** were weak, in rating points;
* the weekly free-agent review asked which **position group** had the lowest-rated deputy, and signed
  anyone above him;
* the renewal and sales reviews asked group counts and a median rating.

The three disagreed. A club with an excellent defence and a poor forward line could spend its one
review on a fourth centre-back because the *bench* defender was the weakest deputy, while the eleven
it actually fielded lost points up front. Nothing ranked the places a club was short in, and nothing
carried a place from one review to the next, so a signing could fill a hole that the next review had
already forgotten — or a club could sell the only cover behind an injured first choice to fund a
different need.

Separately, four AI-club season rows in the ten-season generated-world study reached **31 seniors**
(`docs/APFM_I3_VALIDATION.md` §1 and §5.3). The senior list allows 30. Five paths took a place
without asking the shared gate: a loan that waited to register, an academy promotion, an overage
graduation, a senior lent out whose place was filled while he was away, and the depth pass that
promoted the club's best academy player of a short group. The first three were visible in the I3
record; the last two only showed up once the study was re-run with a diagnostic that names the club
and the route for every row above the limit.

## 2. The plan

`squad_intelligence::plan` turns the model's needs into a **ranked plan**: for each place in the
formation the club is short in, what kind of need it is, how pressing it is, why, and who holds the
place today.

| Field | Meaning |
| --- | --- |
| `slot`, `position`, `group` | where the need is, in the formation the club plays |
| `kind` | `Starter` (the eleven is worse for it), `Cover` (the deputy is), `Succession` (it will be, in a year or two) |
| `reason` | `WeakStarter`, `OutOfPosition`, `WeakCover`, `LongTermInjury`, `AgeingStarter`, `ContractExpiry` |
| `severity` | rating points the place is expected to cost |
| `urgency` | `Optional`, `Important`, `Urgent` — the severity read against the club's `Appetite` |
| `incumbent` | the rating a signing has to beat (the first choice for a weak starter, the deputy for cover) |
| `incumbent_id` | the player who holds the place now, so the renewal and sales reviews can protect him |
| `short_term` | the need matters to the side the club fields this season, not only later |

Urgency is not one threshold per kind. The same gap is more pressing to a club that counts the
present (a contender) than to one that plans two years ahead (a developing club), and less pressing
to a club that asks a lot of strength for its money. The tilt is `squad_intelligence::appetite`,
read from the club's existing strategy profile (`transfers::ClubProfile`: ambition, means, youth
focus) — not a new club classifier.

The plan is recomputed from the squad every time it is read; nothing is cached across a signing, a
sale, an injury or a formation change. It is deterministic: no RNG, and ties break on slot then id.

## 3. How decisions read it

| Decision | Reads | Effect |
| --- | --- | --- |
| Transfer market buyer view | the plan's `Starter` and `Cover` places that matter now | `has_starter_need_for` and `improvement` use the plan's place, so a specialist for a hole in the eleven is not hidden by a deep group; a first choice out for long is a place of its own |
| Weekly free-agent review | the plan's pressing, short-term places, most pressing first | `sign_for_the_plan` tries the weak starter or the deputy's place before the old "weakest cover" fallback; a club with no pressing need still improves its thinnest cover |
| Renewals | `PlanNeed::protects_the_incumbent` | an **urgent** weak-cover or long-injury place keeps its incumbent: a constrained club does not let the only cover go for want of a wage. An ordinary cover stays with the club's own renewal policy; a weak starter is not protected — the plan wants him replaced |
| Sales | the same cover protection | a club does not sell the deputy who holds an urgent cover place to fund a different need |
| Academy promotion, depth promotion, succession promotion, graduation, AI loan | `transfers::signing_place_refusal` | each takes a senior place through the one shared gate; a club at 30 keeps the youngster in the academy, promotes nobody for depth, plans no successor and declines the loan |
| Purchases, free agents, global market | the same gate, unchanged | a place promised to a deal waiting to register — purchase **or loan** — is reserved |

The plan is the planning surface, not a second policy. It does not set wages, fees or contract
lengths; those remain with `ai_finance_guard`, `contract_wage_policy` and the existing negotiation
code.

## 4. The senior-list fix

One authority, `contracts::squad_capacity`, already capped free agents at 30. The leak was in the
routes that did not ask:

* `transfers::registration::signing_place_refusal` now counts **pending loans** as well as pending
  purchases, so two loans cannot each be promised the last place, and a loan registering does not
  block itself;
* it also counts the club's **own seniors out on loan**, whose places they return to: a club that
  lent a senior out and filled the place would be over the limit on the day he came back;
* `youth_pathway::promote_ready_academy_players` stops when the club has no senior place;
* `ai_contracts::graduate_overage_academy_players` leaves an overage youngster in the academy while
  the club is full;
* `transfers::market::try_ai_loan` refuses to borrow for a club with no place;
* `squad_floor::bring_in_one` — the depth pass that promotes the club's best academy player of a
  short group — asks the same gate before it promotes, and signs nobody instead;
* `squad_floor::promote_successor` — the succession promotion — asks it too;
* the manager's **delegated promotion** (`staff_system::assistant::RecAction::PromoteYouth`) asks it
  as well, so the user's club cannot reach 31 through its assistant either;
* a **loaned-in youngster is never promoted or graduated** by the club that borrows him
  (`best_academy_player`, `promote_successor`, `youth_pathway::most_ready`,
  `graduate_overage_academy_players`): he is not that club's to develop, and graduating him would
  hand the parent a senior it never planned for when the loan ends;
* the **manager's own delegated promotion** (`staff_system::assistant::RecAction::PromoteYouth`) asks
  the same gate: a user club can no more register a 31st senior than an AI one, and the refusal is
  the translated squad-full sentence a signing already receives. This is an existing key, so no new
  locale string.

Each route now gives the same answer as a purchase. The study reports no club above 30 seniors
across three seeds and ten seasons.

## 5. Files

| File | Change |
| --- | --- |
| `src-tauri/crates/ofm_core/src/squad_intelligence/plan.rs` | new: ranked plan, urgency, reasons, incumbent ids, tests |
| `src-tauri/crates/ofm_core/src/squad_intelligence/mod.rs` | `ClubSquad::strategic_plan()` |
| `src-tauri/crates/ofm_core/src/squad_intelligence/model/mod.rs` | members carry natural group, contract days and long-injury state; `group_of` |
| `src-tauri/crates/ofm_core/src/transfers/buyer_view.rs` | plan replaces the starter-needs list |
| `src-tauri/crates/ofm_core/src/ai_contracts.rs` | plan-driven improvement and succession; renewal protection; graduation gate |
| `src-tauri/crates/ofm_core/src/ai_squad_sales.rs` | urgent cover is not sold |
| `src-tauri/crates/ofm_core/src/transfers/registration.rs` | pending loans and loaned-out seniors reserve places |
| `src-tauri/crates/ofm_core/src/squad_floor.rs` | depth and succession promotions ask the senior-place gate; a loaned-in youngster is not the club's to promote |
| `src-tauri/crates/ofm_core/src/transfers/market.rs` | AI loan respects the senior place |
| `src-tauri/crates/ofm_core/src/youth_pathway.rs` | academy promotion respects the senior place; a loaned-in youngster is skipped |
| `src-tauri/crates/ofm_core/src/staff_system/assistant.rs` | a delegated academy promotion asks the senior-place gate and is refused with the existing squad-full sentence |
| `src-tauri/crates/ofm_core/src/transfers/tests.rs` | loan/purchase reservation tests |
| `src-tauri/crates/season-harness/tests/i4_strategic_ai.rs` | the career study |

## 6. What this does not do

* It does not plan **fees** across seasons. The market still prices each signing from the club's
  `Appetite` and books; the plan decides *where* to spend, not *how much* to hold back.
* It does not replace `squad_outlook`'s succession arithmetic. `plan_succession` still asks which
  first choices will be worse or gone next year; the plan names the same expiry as a need, and the
  renewal review remains the first answer to it.
* It does not touch the user's own club. Every reader is on the AI side (`ai_contracts`,
  `ai_squad_sales`, the AI buyer's view and the AI loan path).
* It does not change competition generation, fixture scheduling, promotion/relegation, world import
  or the Global Market UI.
* The plan's `ContractExpiry` need is read by the plan's own tests and by the manager-facing plan
  surface; the AI's action on it today is the renewal review and the existing succession path, not a
  new forced signing.

## 7. Cross-branch note (I4B)

I4B owns competition generation, season rollover and the season lifecycle. This change reads only
the current squad, the current date and the current contracts, and is invoked from the existing
daily/weekly AI reviews and the existing registration gate. It adds no new lifecycle hook, so it can
merge before or after I4B without ordering. The one shared seam is
`transfers::signing_place_refusal`: if I4B changes how a season rollover registers players, it must
keep asking that gate (or its replacement) so the senior list stays at 30.
