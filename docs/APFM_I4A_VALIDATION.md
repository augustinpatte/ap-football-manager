# APFM I4A: validation

Evidence and its limits for [`APFM_I4A_STRATEGIC_AI.md`](APFM_I4A_STRATEGIC_AI.md). Branch
`deepseek/i4-strategic-ai`, based on `apfm/main` at `0dc3582dc8a4c0b7b7a87d6746377c7618f90d36`.
Host: macOS, Apple Silicon. Labels: **PASSED**, **FAILED**, **NOT RUN**. A local pass is not a claim
about CI. Nothing was pushed.

## 1. Verdict

**GO** for merging on this branch's evidence, with the follow-ups in §7.1 carried forward. Nothing
below blocks the change; the list is what a reviewer should know it did not prove.

For: the ranked plan exists and is read by the market, the free-agent review, renewals and sales;
every brief-named scenario has a failing-first behavioural test, and three of the new paths were
proven discriminating by removing them and watching the named test fail; the whole `ofm_core` lib
suite and the whole workspace suite are green; the ten-season generated career no longer produces a
club above 30 seniors in any of the three seeds (was 31 in four rows at `0dc3582d`); the study's
squad model improves on every seed.

The honest limits:

1. **The career study is measurement, not a distribution test.** It reports three seeds; a fourth
   seed could show a different leak. The senior list is enforced by one shared gate and tested at
   the gate (§3.1), not by the study alone.
2. **The plan does not plan fees across seasons** (design doc §6). A club short in two places still
   spends on one and comes back next review.
3. **Most senior-list gates were not re-reverted.** Three paths were proven discriminating by
   removing them (§4.3). The academy, graduation and AI-loan gates and the sales and renewal guards
   have tests written failing-first but were not re-reverted for this report.

## 2. Commands run

From `src-tauri/` unless stated. All Rust builds are the workspace's pinned toolchain.

| Gate | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --all -- --check` | **PASSED** (after `cargo fmt --all` applied the branch's own formatting) |
| Clippy (default) | `cargo clippy --workspace --all-targets -- -D warnings` | **PASSED**, 12.7s, no warnings |
| Clippy (MCP) | `cargo clippy --workspace --all-targets --features mcp -- -D warnings` | **PASSED**, 6.1s, no warnings |
| Clippy (all features) | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASSED**, 13.6s, no warnings |
| Backend unit + integration | `cargo test -p ofm_core --lib` | **PASSED**, 2,275 passed, 0 failed, 5 ignored, 33.9s |
| Backend workspace | `cargo test --workspace` | **PASSED**, `EXIT:0`, 5,084 passed, 0 failed, 122 ignored |
| Career study | `I4_SEEDS=1,2,3 I4_SEASONS=10 cargo test --release -p season-harness --test i4_strategic_ai -- --ignored --nocapture` | **PASSED**, `EXIT:0`, 320.7s, 0 rows above the senior limit |

`cargo test --bin` matches zero tests in this tree; the command tests live in the
`openfootmanager_lib` lib target and are covered by the workspace run.

The four commits were also checked one at a time, in a detached temporary worktree sharing the
build cache, so each one is buildable and testable on its own rather than only as a final tree:

| Commit | Check | Result |
| --- | --- | --- |
| `f33058f8` fix(transfers) | `cargo check -p ofm_core --all-targets`; `cargo test -p ofm_core --lib` | **PASSED**, 2,254 passed, 0 failed, 5 ignored |
| `44497b15` feat(ai) | `cargo check --workspace --all-targets`; `cargo test -p ofm_core --lib` | **PASSED**, 2,275 passed, 0 failed, 5 ignored |
| `eba96658` test(ai) | `cargo check --workspace --all-targets` | **PASSED** |
| `6df53316` docs(i4a) | text only | n/a |

## 3. Behavioural tests for the brief's scenarios

Each row names what a reader can run. Every test was written before the change it covers and seen
fail for the predicted reason, except where the row says "control" (a test whose value is that it
fails when the new path is reverted).

| Brief scenario | Test | Where |
| --- | --- | --- |
| Genuine weakness recruitment | `a_club_signs_for_the_weak_starting_place_before_the_weakest_bench` | `ai_contracts.rs` |
| A specialist for a hole in the eleven, not hidden by a deep group | `a_striker_covers_a_first_choice_who_is_out_for_long` | `transfers/buyer_view.rs` |
| No recruitment when the value is insufficient | `a_weak_starting_place_is_not_filled_by_a_player_no_better_than_the_man_there` (the same fixture signs a better striker, so the refusal is the bar, not the fixture); `a_club_with_room_signs_a_free_agent_better_than_its_cover` | `ai_contracts.rs` |
| A fit line is not a need | `a_striker_below_the_first_choice_is_not_a_need_when_the_line_is_fit` | `transfers/buyer_view.rs` |
| Ageing starter replacement | `a_club_with_an_ageing_keeper_signs_his_successor_before_he_retires`, `a_club_with_a_keeper_in_his_prime_signs_no_successor` (control) | `ai_contracts.rs` |
| Expiring contract influence | `an_expiring_first_choice_is_a_planning_need`, `a_first_choice_under_a_long_contract_is_not_a_contract_need` (control), `a_good_player_running_down_his_contract_is_renewed_on_the_review_day` | `squad_intelligence/plan.rs`, `ai_contracts.rs` |
| Financial constraints | `a_club_in_debt_does_not_raise_a_wage_at_renewal_whatever_its_budget_says`, `a_constrained_club_keeps_the_only_cover_for_an_urgent_place`, `a_full_squad_signs_no_one_to_improve_itself` | `ai_contracts.rs` |
| Club differentiation | `two_clubs_read_the_same_squad_differently`, `the_same_gap_is_read_differently_by_clubs_with_different_means`, `urgency_follows_the_club_not_only_the_gap` | `squad_intelligence/plan.rs` |
| The plan updates after a signing | `a_signing_that_fills_the_place_removes_the_need` | `squad_intelligence/plan.rs` |
| Determinism | `the_plan_is_deterministic` | `squad_intelligence/plan.rs` |
| A failed negotiation does not mutate state | `a_renewal_the_player_refuses_changes_nothing` | `ai_contracts.rs` |
| A club does not sell the only cover for an urgent place | `a_club_does_not_sell_the_only_cover_for_an_urgent_place` | `ai_squad_sales.rs` |
| The senior list holds at 30 through every route | `a_club_at_its_senior_limit_promotes_nobody` (academy); `a_club_at_its_senior_limit_promotes_no_one_for_depth` (depth pass); `a_club_at_its_senior_limit_promotes_no_successor` (succession); `a_club_at_its_senior_limit_does_not_graduate_an_overage_youth` (graduation); `a_club_at_its_senior_limit_refuses_a_delegated_promotion` (manager's own assistant); `a_loaned_in_youngster_is_not_promoted_as_the_clubs_own` (ownership); `a_pending_loan_in_reserves_a_senior_place_as_a_purchase_does`, `a_pending_purchase_and_a_pending_loan_are_counted_together` (promises); `a_senior_out_on_loan_reserves_the_place_he_returns_to` (loan out); `a_full_list_with_a_pending_purchase_never_exceeds_the_limit` (existing) | `youth_pathway.rs`, `squad_floor.rs`, `ai_contracts.rs`, `staff_system/assistant.rs`, `transfers/tests.rs`, `ofm_core/tests/transfers_tests/squad_places.rs` |
| Existing protections intact | the full `ofm_core` lib suite, including every prior senior-limit, wage-policy, squad-floor and refusal test, unchanged | `ofm_core` |

### 3.1 The gate is the one place the limit is applied

`transfers::registration::signing_place_refusal` is now asked by purchases, free-agent talks, the
global market, AI squad planning, academy promotion, overage graduation, AI loans and the succession
promotion. It counts three kinds of promise: purchases agreed and waiting to register, loans agreed
and waiting to register, and the club's own seniors out on loan, whose places they return to. The
unit tests above exercise each promise through the function the routes call.

## 4. Before and after

### 4.1 Senior list, three seeds × ten seasons

The baseline is `docs/APFM_I3_VALIDATION.md` §5.1, the same generated world and seeds on the base
commit. "Over 30" counts AI-club season rows whose senior list was above the limit at the rollover.

| Run | Code | Rows above 30 seniors | Worst row |
| --- | --- | --- | --- |
| I3 baseline | `0dc3582d` | 4 | 31 (seed 1 s5; seed 2 s4, s5; seed 3 s6) |
| I4A plan, before the loan-out and depth-promotion reservations | this branch, gate without loaned-out seniors and without the depth promotion | 5 | 32 (seed 2 s2, s5); 31 (seed 1 s5; seed 2 s6, s7) |
| I4A plan, after the loan-out reservation only | this branch, gate counting loaned-out seniors | 3 | 32 (seed 2 s2); 31 (seed 1 s5; seed 2 s5) |
| I4A plan, final | this branch, every promise counted and every promotion gated | 0 | 30 (seed 1 s4, s5; seed 2 s2, s3, s5; seed 3 s3, s4) |

The middle rows are the useful ones. The first cut closed the academy, graduation and AI-loan
routes but left two:

* **the loan-out route** — a club lent a senior out, the freed place was filled, and the loanee came
  back to a full list. Counting the loaned-out senior as a promise closes it, because the club can
  no longer spend the place it has lent away;
* **the depth-promotion route** — `squad_floor::bring_in_one` promoted the club's best academy
  player of a group without asking the gate. A club at 30 could look short in a group because
  `seniors_in` ignores contracts inside the renewal horizon, promote a youngster, and reach 31 with
  no movement entry to show it — which is exactly what the harness's `LimitWatch` observer reported
  for the one remaining over-limit club: 31 seniors, no `LoanReturn` in any of their histories. The
  depth and succession promotions now ask the gate.

The final row is the long-run confirmation; the tests in §4.2 are the regression net.

### 4.2 The evidence at the gate, without the study

`a_senior_out_on_loan_reserves_the_place_he_returns_to` is the regression test for the loan-out row:
28 registered + 1 away means one free place after his return, so one signing is allowed and a second
is refused, and his own return is not blocked by his own reservation.

`a_club_at_its_senior_limit_promotes_no_one_for_depth` is the regression test for the depth row: a
club at 30 promotes nobody for depth, and the same fixture promotes the same youngster once a place
frees. `a_club_at_its_senior_limit_promotes_no_successor` does the same for the succession path.

The study rows above are the long-run confirmation, not the tests. Both regression tests were
verified by removing the guard, in a copy of the tree, and watching them fail (§4.3).

### 4.3 Controlled new-versus-old decisions

Each row removes exactly one new path in a copy of the tree, runs the named test, and restores the
file (verified byte-identical afterwards). A test that passes on both trees proves nothing; these
were seen to fail for the predicted reason.

| New path removed | Test | Predicted | Observed |
| --- | --- | --- | --- |
| `sign_for_the_plan` call in `improve_squad` | `a_club_signs_for_the_weak_starting_place_before_the_weakest_bench` | the old review signs the defender and not the striker | **FAILED**: `assert_eq!(club_of(&game, "striker"), Some("ai"), "the weak eleven")` → `left: None, right: Some("ai")` |
| `signing_place_refusal` guard in `squad_floor::bring_in_one` | `a_club_at_its_senior_limit_promotes_no_one_for_depth` | a full club promotes the academy keeper anyway | **FAILED** at the first assertion (`is_none()`) |
| `signing_place_refusal` guard in `squad_floor::promote_successor` | `a_club_at_its_senior_limit_promotes_no_successor` | a full club promotes a successor anyway | **FAILED** at the first assertion (`is_none()`) |

The first row is the brief's new-versus-old *decision* control, and it is the strongest: the fixture
fields a side whose forward line is 50s against a 60s eleven and whose bench defender is 25, and
offers one free-agent striker of 65 and one free-agent defender of 70. The plan sends the club to
the striker — the place the *filed eleven* is losing points in. Without the plan the striker is not
signed at all, which is what the failure shows; the defender path then takes over. So the test
distinguishes the two trees rather than passing on both.

The other two rows are the senior-list equivalents: each guard is the only thing standing between a
full club and a 31st senior on that route.

### 4.4 Squad model, three seeds × ten seasons (final run)

79 AI clubs per seed (the user's club is excluded), generated world, 10 seasons, release build.
The run reported here is on the exact committed tree: it was repeated after the last refactor (the
plan split into named passes) and reproduced the per-season table byte-for-byte, which is why the
figures below are the committed ones.
`XI fit` is the mean rating of the eleven the club would field, on the same 0–100 scale as a player.
`deals need` is a signing into a group below the squad floor (fewer seniors than the floor);
`full` is a signing into a group already at the depth the club plans for; the rest is genuine depth.
`gain` is the mean change in the eleven's rating that one signing produced.

| Seed | XI fit s1 → s10 | joiners/club/season | deals | into need / full | mean age | ≤21 / ≥32 | fees M/club/season (s1 → s10) | mean gain |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 68.9 → 71.5 (+2.6) | 1.3 → 2.9 (2.5 mean) | 472 | 4% / 57% | 21.8 | 57% / 4% | 0.05 → 1.62 (0.85 mean) | +0.32 |
| 2 | 69.2 → 71.3 (+2.1) | 1.4 → 2.3 (2.6 mean) | 569 | 3% / 63% | 21.7 | 58% / 5% | 0.05 → 1.09 (0.96 mean) | +0.32 |
| 3 | 69.3 → 71.4 (+2.1) | 1.2 → 2.2 (2.6 mean) | 485 | 2% / 57% | 22.3 | 52% / 6% | 0.01 → 1.18 (0.85 mean) | +0.32 |

Squad size and the books, from the same run (per-club medians, season 10):

| Seed | seniors min/mean/max | thin groups | no GK / one GK | in debt | wages >110% of budget | wage/budget P50 | cash P50 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 20 / 23.9 / 28 | 0 | 0 / 0 | 0 | 0 | 1.00 | 45.3M |
| 2 | 20 / 23.8 / 28 | 0 | 0 / 0 | 0 | 0 | 1.00 | 46.9M |
| 3 | 20 / 23.8 / 28 | 0 | 0 / 0 | 0 | 0 | 1.00 | 46.6M |

No club in any seed was in debt at season 10. Wage budget P50 of 1.00 is the ceiling the wage
policy targets, not an overspend: `in debt` and `>110% wages` are the failure measures and both are
zero at the last snapshot. One club crossed 110% in season 3 of seeds 1 and 2 and was back under by
season 4.

## 5. What the study says

Measured on the final code, three seeds × ten seasons, 79 AI clubs a seed (237 club-seasons a seed,
790 in all). The I3 record (`docs/APFM_I3_VALIDATION.md` §5.1) reports the senior-list maximum and
the invariant result only, not squad strength or recruitment, so the strength comparison below is
within this run (season 1 → season 10) rather than I3 → I4A.

**Recruitment by need.** Of 1,526 completed signings, 2–4% went into a group below the squad floor
and 57–63% into a group already at the depth the club plans for. The pattern is what a planning club
should produce: the floor is fixed early, so signings are overwhelmingly quality upgrades to an
established group rather than emergency bodies. The `need` figure being *low* is the good direction —
a high one would mean clubs spend the window plugging critical holes. What this metric cannot say is
whether the upgrade went to the club's weakest *starting place*; that is the job of the controlled
test in §4.3, not of a group-depth count.

**Squad improvement.** Mean XI fit rose in every seed: 68.9 → 71.5, 69.2 → 71.3, 69.3 → 71.4, all
about +2 points on a 0–100 scale over ten seasons, with the mean gain per signing positive in every
single season of every seed (+0.22 to +0.62). No seed bought its way to strength: fees rise from
~0.05M to 1.1–1.6M per club per season as prize money arrives, and the mean per-signing gain stays
positive, so the money bought improvement rather than volume.

**Duplicate signings.** 14% of joiners over the run never made the eleven (`unused joiners`), and it
splits by route: 27–31% of permanent transfers were unused against 8–10% of free agents. The
transfer market is where a club buys depth it does not field; the free-agent review, which is
plan-driven, is the better-targeted route. Squads never stacked: the mean senior count stayed
between 22.3 and 24.9 across the run (23.7–23.9 at season 10) and never exceeded 30 in any club in
any season, so no club hoarded players.

**Age distribution.** Mean age of a signing 21.7–22.3; 52–58% were 21 or under and 4–6% were 32 or
over. Clubs buy young and do not spend the window on veterans — the ageing-starter route is served
by succession (own academy first), which is why the ≥32 share stays low.

**Squad size and shape.** Seniors 20 / 23.8–23.9 / 28 (min/mean/max) at season 10; zero thin groups;
zero clubs with no goalkeeper or only one; usable bench 11.7. Every club a season can field a
formation.

**Contract outcomes.** Renewals are the main way a club keeps a squad together: 4.0–5.1 renewals a
club a season at season 10, each adding +42–45% on the renewed wage, but booked only against room
under the budget. Every renewal is an offer the player can refuse, and a refused renewal changes
nothing — his contract, wage and club are as they were, so the plan still sees the place he holds
(the regression test in §3).

**Finance.** No club was in debt at the season-10 snapshot in any seed, and no club was above 110% of
its wage budget at that snapshot (one club in seeds 1 and 2 crossed it in season 3 and was back under
by season 4). Wage/budget P50 of 1.00 is the policy's own ceiling, not an overspend. Cash P50 at
season 10 was 45–47M and operating result stayed positive in every tier (Q1 1.6M to Q4 3.4M in seed
1), so the plan's spending is funded by income rather than borrowing. Wage-adding events are mostly
within budget: at season 10, 151 free-agent signings, 96 transfers, 385 renewals and 100 youth
promotions were within budget against 17, 2, 20 and 64 over it. The 64 youth promotions over budget
are the largest remaining leak in the books — an academy intake is budgeted after the fact, not
refused (§7.1).

**Runtime.** 320.7 s for three seeds × ten seasons in a release build (106.1 / 106.8 / 107.8 s a
seed, 7.7–11.9 s a season). Per-season time grows slowly across a career (7.7 s in season 1 to about
11–12 s by season 9) as squads and the deal log fill; no seed slowed down or stalled.

## 6. Runtime and memory

- Study: 320.7 s wall time for 3 seeds × 10 seasons, release profile, single-threaded.
- Backend lib suite: 33.9 s (2,275 tests, 5 ignored); the whole workspace suite took about 11 minutes
  including the root app's SQLite and match tests.
- Peak memory was not instrumented; the run holds one generated world (about 3,700 players) and the
  deal log at a time.

## 7. Not run, and limits

- **No frontend or browser work.** The plan has no screen of its own; the manager-facing squad plan
  surface already existed and its tests are unchanged. No locale string was added, so the locale
  coverage gates are untouched (frontend suite NOT RUN for this change).
- **No manual play.** The long career is the harness, not a person.
- **One design seam with I4B.** `transfers::signing_place_refusal` is the shared gate. I4B owns
  competition generation, rollover and the season lifecycle; if it registers or releases players on
  a new path it must ask this gate (or its replacement) for the 30-senior invariant to keep holding.
  No ordering between the branches is required: this change adds no lifecycle hook.
- **The plan reads, it does not cache.** Every reader recomputes it from the squad; a long career's
  runtime (§6) is the cost, and it is paid by the AI review cadence, not by every day.
- **The study is evidence, not a test.** It asserts nothing about distributions; a regression in
  recruiting quality would show as a worse table, not a red test. The behavioural tests in §3 are
  the regression net.

### 7.1 Weaknesses left open

- **The transfer market still buys unused depth.** 27–31% of permanent transfers never made the
  eleven, against 8–10% of free agents. The plan targets the free-agent review well; the cash
  market is still driven by availability and price, not by the plan's weakest place.
- **The plan does not ration fees across reviews.** A club short in two places buys one and returns
  next review with whatever is left, which is why a club can sign into a group already at planned
  depth while a starting place is still weak.
- **A weak starting place is replaced only when a better player is available.** The plan sets the
  bar (the incumbent's rating) but does not manufacture a target; a window with no better striker
  leaves the place weak, by design and by honesty about the market.
- **Youth intake is not refused for wages.** At season 10, 64 youth promotions were over the wage
  budget against 100 within it — the largest remaining book leak. The intake is budgeted after the
  fact. Out of scope here (it is not a signing into the squad's plan).
- **The user can still promote a player who is only on loan at the club.** `RecAction::PromoteYouth`
  now asks the senior-place gate, so a full club cannot reach 31 through its assistant. It does not
  check ownership: a manager who deliberately promotes a *loaned-in* youngster gives him a senior
  role that travels back to his parent when the loan ends, and the parent could then be one over.
  No AI path can do this (every AI promotion and graduation filters loaned-in players), and refusing
  it would need a new translated sentence, which this assignment may not add. Noted for whoever owns
  the assistant UI.
- **Three seeds is not a distribution.** The career study is three generated worlds; another seed
  could surface a route the gate does not cover. The gate itself is unit-tested per promise (§3.1),
  which is the stronger claim.
