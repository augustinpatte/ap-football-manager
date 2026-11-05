# APFM Run F5.3 — 30-year club finances and tactical pressing

Branches: OFM `claude/run-f5-3-finances-and-pressing` (from F5.2's `c1a15884`), M12 `f5.3/pressing` (from `4679aaf4`).
Two defects, both proven in F5.2 and both inside the authorised scope: the shipped 440-club world's clubs sink into
debt over thirty seasons, and a press at 99 dominates every tactical matchup. **Both are corrected and measured; two
named defects remain inside the same scopes (§14).**

Verdict: **GO for the two objectives of this run; the NO-GO for beta stands** (no GUI playthrough, ageing and the
event-rate items F5.2 listed are untouched, §17).

## 1. Verified starting commits

| Repository | Branch at start | HEAD | Working tree |
|---|---|---|---|
| OFM | `claude/run-f5-2-football-realism-and-ai` | `c1a15884` (the report commit, as the report says) | clean |
| M12 | `f5.2/football-realism` | `4679aaf4…` = the revision OFM pins (`ofm_core/Cargo.toml`) | `Cargo.toml` licence edit and untracked `LICENSE`, `NOTICE`, **left as found, never staged** |
| Viewer | `main` | not touched | `package.json`, `LICENSE`, `NOTICE` modified/untracked, not touched |

The F5.2 pin was checked against the real git revision with the local `paths` override moved aside (`cargo check -p ofm_core`),
and the new pin was checked the same way after the push (§15). Personal saves, original datasets and the viewer were not touched.

## 2. Exact financial root causes

**Opening books are coherent; the world is made unsustainable afterwards.** The shipped world's first season is profitable in every
standing (mean operating result +1.7 M for the poorest quarter, +12.5 M for the richest; wage bills 55–71 % of income; 41 of 439 clubs already above 110 % of
their budget by the end of it in F5.2, none in F5.3).
The suspected mismatch between a reputation floor of 400 and shipped reputations from about 300 is real in the dial (the poorest quarter earns
5.8 M a year, the 0.4 floor) but it is **not** what sinks the clubs: those clubs open at 71 % of income on wages and are in profit.

**What happens next, measured by club standing** (new instrument, §3; shipped world, seed 1, F5.2 code): revenue is flat in nominal terms,
wages are not.

| founding reputation quartile | wages s1 → s10 → s30 (M) | income (M) | wage bill ÷ income | clubs in debt s30 | wage bill ÷ budget s30 |
|---|---|---|---|---|---|
| Q1 (lowest) | 3.3 → 7.6 → 9.1 | 6.3 | 0.70 → 1.26 → 1.47 | 97 / 110 | 2.00 |
| Q2 | 4.3 → 9.6 → 10.2 | 8.8 | 0.65 → 1.07 → 1.15 | 69 / 110 | 1.59 |
| Q3 | 7.6 → 13.7 → 14.1 | 14.8 | 0.63 → 0.94 → 0.93 | 10 / 110 | 1.19 |
| Q4 (highest) | 12.5 → 20.8 → 21.0 | 24.9 | 0.58 → 0.86 → 0.84 | 0 / 110 | 1.12 |

Sixteen nations, 440 clubs, season 30 in F5.2: 20 of 20 Irish and Northern Irish clubs and 19 of 20 Welsh clubs in debt, against 1–5 of 40 in Spain, Germany and
Italy. The nations the 80-club gate world uses (England, Portugal, Argentina) are the rich end of the pyramid; the gate world did show debt in F5.2 (7, 12 and 14 of 79
at season 30, §5) but at a third to a half of the shipped world's rate (9–18 % against 40 %).

**Where the wage growth comes from** — a daily ledger of every event that added to a club's weekly wage bill, split by whether the club was over its
budget once the event had happened (gate world seed 1, F5.2 code):

| season | over budget after the event (wage added a week) | within budget |
|---|---|---|
| 5 | free agents +1.96 M, renewals +1.64 M, youth +1.09 M, transfers +0.87 M (5.71 M in all) | +0.17 M |
| 10 | free agents +1.73 M, renewals +1.69 M, youth +1.36 M, transfers +1.21 M (5.99 M in all) | +0.01 M |
| 20 | free agents +3.00 M, renewals +1.58 M, youth +1.64 M, transfers +1.28 M (7.51 M in all) | +0.2 M |

**Essentially every wage added in the F5.2 world was added by a club that was already over its budget.** Four decision paths let it (each found
by reading the path, then confirmed by the ledger):

1. **`ai_finance_guard`: a grace granted at every decision.** An AI club already over its wage budget could add `max(3 % of the budget, …)` at
   *each* signing and renewal. The 3 % looks proportional; granted again to each of a dozen contracts a year it is a licence to grow by a third a year.
   Lower-nations seed 1: stayers renewed at **+37 %** on average (`ai_contracts::renew`).
2. **`youth_intake`: the board's policy on top of an academy ratchet.** A recruit's ceiling was a quarter above what the academy already pays, so each
   intake could be paid a quarter more than the last, and the policy's 25,000-a-week grace for an over-budget club let every one through. Academy
   youngsters were paid like the first team (5,800 a week against seniors on 6,000 at a lower-nations club): about **a half of the wage bill** and,
   in the ledger, +1.1 M a week of wage added per season over 79 clubs.
3. **`squad_floor::best_signing`: a pay scale that ratchets.** What a club could "afford" was `max(room under the budget, 1.5 × its median wage)`:
   a club at the budget could still take anyone up to one and a half times what it paid in the middle, so the median rose with each signing.
   Free agents joined at 8,000–17,000 a week at clubs whose median was 3,500 (a free agent also keeps the wage he last earned, with a premium).
4. **The budget review cannot catch a bill.** The yearly review cuts a budget by at most 5 % (15 % in debt) while nothing sheds the bill; budgets fell to
   the income target while bills rose: median bill ÷ budget 2.00 for the poorest quarter at season 30, **371 of 439 clubs over 110 %** (F5.2: 411).

Loans did not appear in the ledger (no `LoanStart` events added wage over budget); paid transfers were a minor path once the other four were closed
(+0.8 k a week at season 10 in the final code).

## 3. Financial code changes

| File | Change |
|---|---|
| `ofm_core/src/ai_finance_guard.rs` | No grace for an AI club. `AI_WAGE_CEILING_PCT = 100`: an AI club's own decisions stop at its **budget** (the board's 110 % is for a manager), a club past it adds nothing; `room_for_wages()` is the one definition of what is left. |
| `ofm_core/src/ai_contracts.rs` | `renew` offers what fits (`affordable_renewal_wage`: the ask, held to the room, never below what he is paid); a non-content player offered less than he asked is judged as any renewal is (he may leave); a club with cash below zero raises no wage; a club with room pays a player whose pay fell behind his value what his value implies (`quality_wage_catch_up`, in `contracts/helpers.rs`). |
| `ofm_core/src/contracts/helpers.rs` | `insulting_floor` (shared with the existing insult test), `free_agent_bid`, `quality_wage_catch_up`. |
| `ofm_core/src/squad_floor.rs` | `best_signing` bids what the club can carry, never below the insult line; the pay-scale allowance and `median_senior_wage` are gone. A club short of its floor still signs, at the lowest bid. |
| `ofm_core/src/youth_intake.rs` | A recruit asking more than the youth minimum must fit an AI club's books; one at the minimum is never refused. (A first attempt that held the minimum to the books too, and a second that capped academy pay at half the seniors', each removed debt and each had a side effect the study caught: population 14,000 → 10,300, and wages collapsing to 30 % of income. Both were reverted; §4.) |
| `season-harness` | `finance_study` (the instrument, outside every save), `WorldSpec::lower_nations` (the four poorest nations, a 80-club diagnostic that reproduces the shipped world's debt in 75 s), `tests/f53_finance_study.rs` (measurement), `tests/f53_financial_stability.rs` (regression). |
| `docs/GAME_SYSTEMS.md` | "How an AI club keeps its wage bill". |

The player's own club keeps the board's policy; nothing here applies to it. No serialized field was added (nothing in a save changes shape).
Existing saves load; an AI club that was over its budget simply cannot grow its bill and sheds it as contracts end.

**Why the catch-up.** With the ratchets closed the poorest clubs are solvent, but a wage that only follows the wage a player was paid last
never follows his rating: rich clubs then used 60–70 % of their wage budgets, small clubs 100 %, and in the gate world the strongest founding quarter
of clubs fell from the 0.75 strength percentile to 0.46–0.60. An experiment on the catch-up (a temporary environment switch, removed) gave, for a cap of
+50 % / +200 % / the full value: bill ÷ budget 0.64–0.70 / 0.98 / 1.00 and big-club percentile 0.58 / 0.43–0.55 / **0.64–0.69**. The full value, still
held to the room under the ceiling, is what shipped.

## 4. Full-world financial results, before and after (identical world seeds)

Shipped world (440 clubs, 16 nations, verified before every run: `== seed N: 440 clubs`; the user's club is excluded from the counts, 439 clubs),
three seeds, thirty seasons, F5.2 code (built in a separate worktree at `c1a15884` with the instrument added) against F5.3 code:

| season | seed | clubs in debt | wage bills over 110 % of budget | cash P10 / P50 / P90 (M), F5.2 | cash P10 / P50 / P90 (M), F5.3 | mean operating result a club-year (M) |
|---|---|---|---|---|---|---|
| 5 | 1 | 0 → **0** | 323 → 0 | 5.9 / 13.3 / 39.5 | 7.6 / 17.4 / 54.7 | +1.2 → +2.7 |
| 5 | 2 | 0 → **0** | 332 → 0 | 5.9 / 13.0 / 42.4 | 7.3 / 18.4 / 52.8 | +1.2 → +2.7 |
| 5 | 3 | 1 → **0** | 344 → 0 | 5.9 / 12.9 / 43.9 | 7.4 / 17.6 / 54.2 | +1.1 → +2.7 |
| 10 | 1 | 26 → **0** | 380 → 0 | 1.8 / 13.1 / 51.2 | 10.4 / 26.3 / 80.3 | −1.1 → +1.2 |
| 10 | 2 | 18 → **0** | 368 → 1 | 1.9 / 13.3 / 52.2 | 10.0 / 26.1 / 79.7 | −1.1 → +1.2 |
| 10 | 3 | 21 → **0** | 364 → 0 | 1.5 / 13.3 / 54.7 | 10.4 / 26.4 / 82.2 | −1.1 → +1.2 |
| 15 | 1 | 83 → **0** | 357 → 1 | −1.8 / 10.7 / 58.3 | 13.1 / 37.0 / 104.3 | −0.4 → +2.1 |
| 15 | 2 | 69 → **0** | 363 → 0 | −1.2 / 11.3 / 58.6 | 12.9 / 36.7 / 106.7 | −0.5 → +2.0 |
| 15 | 3 | 80 → **0** | 363 → 0 | −2.0 / 11.2 / 63.0 | 13.0 / 35.4 / 112.4 | −0.4 → +2.1 |
| 20 | 1 | 116 → **0** | 364 → 0 | −5.1 / 8.5 / 68.5 | 16.9 / 47.4 / 130.5 | −0.5 → +2.1 |
| 20 | 2 | 112 → **0** | 372 → 0 | −5.2 / 8.7 / 62.0 | 15.5 / 47.1 / 131.6 | −0.6 → +2.1 |
| 20 | 3 | 122 → **0** | 383 → 0 | −4.8 / 9.0 / 75.4 | 16.0 / 46.4 / 139.2 | −0.6 → +2.2 |
| 25 | 1 | 150 → **0** | 363 → 1 | −10.1 / 5.8 / 71.5 | 19.1 / 54.8 / 158.0 | −0.6 → +2.2 |
| 25 | 2 | 144 → **0** | 372 → 0 | −11.9 / 7.2 / 69.6 | 18.6 / 57.9 / 159.7 | −0.8 → +2.1 |
| 25 | 3 | 141 → **0** | 363 → 0 | −11.7 / 5.5 / 73.9 | 18.9 / 57.1 / 165.6 | −0.6 → +2.0 |
| 30 | 1 | 176 → **0** | 371 → 2 | −19.5 / 4.0 / 71.2 | 21.9 / 66.2 / 185.4 | −1.7 → +1.2 |
| 30 | 2 | 170 → **0** | 379 → 2 | −21.4 / 3.7 / 74.7 | 21.9 / 67.3 / 185.7 | −1.7 → +1.2 |
| 30 | 3 | 172 → **0** | 375 → 1 | −22.4 / 3.8 / 81.1 | 22.3 / 67.5 / 194.1 | −1.6 → +1.2 |

(Season 0 is 0 in debt in both. The F5.2 columns are the F5.2 code re-run with the instrument in a separate worktree; the F5.2 report's own run of seed 1 read 174 of 440, this run
reads 176 of 439 AI clubs: the instrument counts the 439 clubs the user does not run.)

**Acceptance ranges, and why these.** The model's own identity is `operating result = income − wages − upkeep`, and the wage budget is a share of
income (80 % for the smallest club falling to 55 % for the largest). A sustainable club therefore has a wage bill at or below its budget and a
non-negative result over a decade. Ranges used: clubs in debt ≤ 5 % of the world at every checkpoint (not zero: a bad run of results can put a club
briefly below zero); clubs over 110 % of budget ≤ 5 %; cash P10 ≥ 0 from season 10; mean result ≥ 0. *Temporary debt* is cash below zero with a
non-negative operating result; *insolvency* is cash below minus one year of income. F5.2 failed the first from season 10 (26 of 439 = 6 %, then 19 %, 26 %, 34 %, 40 %) and
the others from season 15 (its P10 was −19 to −22 M at season 30, three to four years of a poor club's income); F5.3 passes all four at every checkpoint on all three seeds, and **no club is in debt at all**, so
neither temporary debt nor insolvency occurs. "Zero" is a result, not a target: the regression allows 10 %.

The cash that accumulates (median 66 M at season 30, from 3.4 M) is the price of the budget rule: clubs spend up to the budget (bill ÷ budget 1.00
at the median) and the budget is a share of income, so the margin accrues. The rich quartile holds 167 M; no club's cash was reset, injected or
subsidised, and no club is named in the rules.

## 5. Thirty-season results across seeds and by standing

Seed 1, season 30, by founding reputation quartile (F5.2 → F5.3):

| quartile | clubs in debt | cash P50 (M) | mean result (M) | bill ÷ budget | clubs over 110 % | wage bill ÷ income |
|---|---|---|---|---|---|---|
| Q1 | 97 → 0 | −14.9 → 27.4 | −2.9 → −0.1 | 2.00 → 1.00 | 108 → 0 | 1.47 → 0.91 |
| Q2 | 69 → 0 | −2.3 → 49.9 | −1.8 → +0.5 | 1.59 → 1.00 | 108 → 2 | 1.15 → 0.86 |
| Q3 | 10 → 0 | 9.4 → 81.2 | −1.1 → +1.4 | 1.19 → 1.00 | 91 → 0 | 0.93 → 0.79 |
| Q4 | 0 → 0 | 63.8 → 166.8 | −1.0 → +3.1 | 1.12 → 1.00 | 64 → 0 | 0.84 → 0.76 |

The poorest clubs run at about break-even (−0.1 M) — they survive on sound management and no more. By country (season 30, seed 1, F5.3): all sixteen
nations have 0 clubs in debt; median cash 20.6 M (Northern Ireland) to 153 M (England); the only nations with a negative mean operating result are Belgium (−0.1), Croatia (−0.2),
Scotland and Sweden (−0.3), Ireland (−0.4), Wales (−0.5) and Northern Ireland (−0.6 M a club-year), i.e. the world's poorest are the ones with a thin margin, as they should be.

**Gate world (80 clubs), the regression check, F5.2 → F5.3, three seeds, season 30:** clubs in debt 7 / 12 / 14 → **0 / 0 / 0**; clubs over 110 % of budget
68 / 58 / 60 → 0 / 0 / 0; cash P10 +1.0 / −5.7 / −3.9 M → +61.7 / +65.8 / +51.5 M.
**Lower-nations world (80 clubs, the four poorest nations), seed 1:** 73 of 79 in debt at season 30 → **0**, over budget 77 → 0.

**Squad viability, population and market (shipped, seeds 1–3, F5.2 → F5.3 at season 30):** clubs thin in a position group 107 → 1–3; squads 20.4 [12–25] → 23.7
[13–29]; clubs under 16 seniors 1 → 1; population 14,751 → 14,750–14,806 (the F5.2 level); free agents 3,604 → 1,733–1,835.
**Transfer activity, seed 1, thirty seasons (F5.2 → F5.3, `f51_world_health`, 747 s):** paid transfers 10,145 → 10,619 and fees 12.5 bn → 14.2 bn (the market is as busy and
dearer); free signings 41,618 → 66,073, releases 35,672 → 54,091, renewals 72,398 → 66,684 (more churn: clubs that cannot raise a wage let the player go and sign another); mean age of
players at clubs at season 30 21.7 → 23.3.

**Major clubs vs small clubs.** *Financial* advantage is intact: the rich quartile earns 3.6 × the poor one's income (23.7 M against 6.5 M), holds 6 × its cash and
spends 3 × its wages. *Sporting* advantage is narrower than in F5.2, because small clubs now keep their squads: across the three shipped seeds at season 30 the
correlation of reputation with squad strength is 0.60–0.66 (F5.2: 0.63–0.67, unchanged), the founding top quarter sits at the 0.61–0.66 strength percentile
(F5.2: 0.74–0.76) and the founding bottom quarter at 0.37–0.39 (F5.2: 0.29–0.33). In F5.2 the small clubs were weak partly because they were bankrupt.

## 6. Wage-budget enforcement evidence

* The ledger of §2, gate world seed 1, wage added a week by events that left the club over budget: season 5: 5.71 M → 0.67 M; season 10: 5.99 M → 0.29 M;
  season 20: 7.51 M → 0.41 M (events within the budget are now where the growth is: 4.75 M a week at season 10, spent on free agents, renewals and transfers the club can carry).
* Clubs over 110 % of their budget: 371 → 2 (§4).
* Per-path tests (`ofm_core`): `a_club_over_its_wage_budget_adds_no_wage_at_all`, `the_room_for_wages_is_what_is_left_under_the_budget`,
  `a_renewal_is_offered_what_fits_under_the_ceiling_and_never_less_than_he_is_paid`, `a_club_at_its_budget_does_not_raise_an_unhappy_players_wage_at_renewal`
  (at the wage limit), `a_club_with_room_under_its_budget_raises_an_unhappy_players_wage_at_renewal` (positive cash, the control),
  `a_club_in_debt_does_not_raise_a_wage_at_renewal_whatever_its_budget_says` (debt), `a_club_bids_the_room_it_has_for_a_free_agent_who_asks_more`,
  `a_club_below_its_floor_with_no_room_signs_at_the_lowest_bid`, `an_ai_clubs_recruit_has_to_fit_its_books_unless_he_comes_at_the_minimum`,
  `a_club_with_room_pays_a_star_whose_pay_fell_behind_his_value_and_a_club_at_its_budget_does_not`, the helpers' bid and catch-up tests, and
  `season-harness/tests/f53_financial_stability.rs` (two lower-nations worlds, ten seasons: ≤ 10 % in debt, ≤ 15 % over 110 %; the F5.2 code ends on 27 of 79 in debt).
* **Mutation testing, one mutant at a time, all killed** (`ofm_core --lib`, restored after each): renewal always offers the wanted wage (3 tests fail); bid
  without the insult floor (3); youth minimum held to the books (1); renewal of a club in debt allowed (1); catch-up unbounded by its test (2); AI ceiling disabled
  (`AI_WAGE_CEILING_PCT = 100_000`): the stability regression fails with 41 of 79 clubs in debt at season 10.
* Not changed and not re-measured on their own: loans in (no wage-adding events in the ledger), the manager's and the assistant's renewals (the board's policy, untouched).

## 7. Pressing root causes

Measured on identical-strength career inputs (40 OFM-adapted matches × seeds, home side's tactic against a neutral away side, F5.2 football), with per-side figures
the F5.2 panel did not have (new in `examples/f52_realism.rs`):

1. **The effort model charges a press only for its metres.** A press at 99 covered +5 % distance and +10 % sprints and ended the match at a long-term fatigue of
   0.38 against 0.36: *the slider had no physical cost* (F5.2 §4).
2. **No preset turns the M10 pursuit controls on** (`m10.press.enabled` is false in every production configuration): a press has no leash, no trigger distance, no
   commitment and no rest. Pressing is `range = 8 + 22 p` and one, two or three pressers, permanently. The cooldown code that halves the rest "when the team presses
   hard" is unreachable. Enabling it is not a fix: it takes the engine to 4.0 goals a match (measured), an unrelated calibration regime.
3. **The benefit is suppression, not turnovers.** Possession changes are identical at neutral and 99 (71 and 70 regains), high regains 30.3 against 29.8; what changes is the
   opponent's shots, 10.9 → 8.9 (−18 %), at no cost to the presser's own (11.0 → 10.5).
4. **The other end of the slider has no defence.** At pressing 15 the opponent shot 15.9 times against 11 (win − loss −14 against neutral): one defender may close the
   carrier within 11 m and nobody else does, wherever the ball is — including in the team's own box. The low block (press 15, line 20, mentality 30) lost 20 / 25 / 55.

## 8. Pressing code changes (M12 `28f34b1`, flag `f53`)

`FootballConfig::full_match_f53() = full_match_f52() + m12.press_costs`; every field is zero/off in every earlier mode and has a `serde(default)`, so a config saved
before the costs existed loads unchanged (tested) and `full_match_f53` minus its costs **is** `full_match_f52` (tested).

| Mechanism | What it does | Final value |
|---|---|---|
| `chase_effort` | energy multiplier `1 + k·pressing` while a defender holds a pressing label (press / contain / cover) — the braking and turning a point-mass model does not see | 1.0 |
| `stance_effort` | `1 + k·(pressing − 0.5)` on every outfield player's energy while his side does not have the ball (a squeezing team runs more, a deep block less) | 0.6 |
| `stretch_m` | midfield and forwards step up (press 100) or drop (press 0, at `drop_scale` = 0.3 of that distance) while out of possession; the back line still follows `line_height` | 14 m / 0.3 |
| `box_floor` | the effective press rises from the slider to 0.35 as the ball comes from 30 m to 12 m of the team's own goal: the slider decides where a team starts to press, not whether it defends its box | 0.35 over 30 → 12 m |

Values were set by dose-response on the realism panel: a floor of 0.7 fixed the low block but took neutral-v-neutral from 2.23 to 2.00 goals; one at the neutral 0.5 left neutral matches
bit-identical but cost the career inputs 0.27 goals and 4.9 shots (many AI teams press below 0.5); 0.3 changed nothing; 0.35 is the step at which a second defender joins the press, and the
smallest that moves the low block. The zone (30 m to 12 m) and the stretch (14 m) were then widened/narrowed to give back most of the goals the floor takes. One defect in an earlier version
was caught by the measurement: the chase cost was gated on a field that only exists with the M10 controls, so it never fired (`chaseonly` reproduced F5.2 exactly).
`full_match_f53` is what OFM plays: `spatial_runner::match_football_config()` (instant and interactive matches alike), M12 re-pinned to `28f34b1`, and a match or checkpoint
saved under F5.2 replays under F5.2 (`a_checkpoint_recorded_under_f52_is_restored_and_stays_f52`).

## 9. Tactical benchmark, before and after (`examples/f52_realism.rs`, same inputs and seeds, 320 matches a row; standard error of a win − loss figure ≈ ±7.5)

Home plays the first-named style against the second (identical-strength teams, every other slider 50). "Low block" = pressing 15, line 20, mentality 30;
"counter" = mentality 35, line 30, pressing 30, directness 70, tempo 65; "possession" = directness 20, tempo 40, width 60; "direct" = directness 85, tempo 70.

| home v away | F5.2 W/D/L | win − loss | F5.3 W/D/L | win − loss | home fatigue F5.2 → F5.3 |
|---|---|---|---|---|---|
| neutral v neutral | 34/29/37 | −3 | 33/34/33 | 0 | 0.36 → 0.39 |
| press 99 v neutral | 37/33/29 | **+8** | 31/32/37 | **−6** | 0.39 → **0.59** |
| press 70 v neutral | 38/27/35 | +3 | 37/31/32 | +5 | 0.39 → 0.49 |
| press 15 v neutral | 30/26/44 | −14 | 38/26/37 | +1 | 0.32 → 0.27 |
| press 99 v low block | 58/24/18 | **+40** | 39/26/36 | **+3** | 0.39 → 0.62 |
| press 99 v possession | 38/35/28 | +10 | 28/35/38 | **−10** | 0.40 → 0.60 |
| press 99 v direct | 46/29/25 | +21 | 32/30/38 | **−6** | 0.37 → 0.56 |
| press 99 v counter | 65/19/15 | **+50** | 52/21/27 | **+25** | 0.37 → 0.57 |
| press 99 v high line (85) | 35/27/38 | −3 | 29/30/41 | −12 | 0.39 → 0.58 |
| press 99 v press 99 | 37/36/28 | +9 | 38/33/30 | +8 | 0.38 → 0.56 |
| press 99 + line 85 v neutral | 41/31/28 | +13 | 35/26/39 | −4 | 0.43 → 0.64 |
| press 99 + line 85 v low block | 63/22/15 | **+48** | 49/25/26 | **+23** | 0.41 → 0.63 |
| low block v neutral | 20/25/55 | −35 | 27/25/48 | −21 | 0.27 → 0.23 |
| counter v neutral | 20/21/59 | −39 | 23/27/50 | −27 | 0.32 → 0.30 |
| direct v neutral | 33/29/38 | −5 | 30/27/43 | −13 | 0.37 → 0.41 |
| attacking (mentality 85) v neutral | 35/28/36 | −1 | 30/30/40 | −10 | 0.39 → 0.42 |
| defensive (mentality 15) v neutral | 35/26/39 | −4 | 29/29/41 | −12 | 0.36 → 0.39 |

The three last rows move by −8 to −12 although those sides' pressing is the neutral 50: with a shared neutral opponent and shared seeds the noise is correlated and
the shift is not distinguishable from sampling error at this size (±7.5 each); they were not tuned and are reported as measured.

**Unequal strength** (home +6, away −6 on every outfield attribute, 240 matches): the stronger side wins 72 / 17 / 11 against a neutral weaker one; when the *weaker* side presses at 99
it wins **6 %** (F5.2: 9 %) and loses 83 %: maximising the press does not beat a stronger team. A stronger side pressing at 99 wins 67 % (F5.2 75 %), a little less than at neutral — the
press costs it.

## 10. Pressing fatigue and counterplay evidence

* **Fatigue.** A press at 99 now ends the match at 0.59 long-term fatigue against 0.38 for the neutral side opposite (F5.2: 0.39 against 0.36); a press at 70 at 0.49; a press at 15 at 0.27.
  `tests/f53_press_costs.rs::a_high_press_is_paid_for_in_fatigue_under_f53` asserts presser > F5.2 presser + 0.05 and > press-15 team + 0.10 (killed by removing the effort costs).
* **Late-match performance.** Open-play shots per 30 minutes, press 99 v neutral: the presser 3.19 in the first hour, 2.69 in the last half hour (−16 %); the neutral side 3.20 then 3.29. In F5.2 the
  opponent's rate *fell* in the last half hour (3.01 → 2.76) while the presser's fell 14 %: a tired press no longer suppresses the other side.
* **Exposure and counterplay.** The stretch is what a team that bypasses the press finds: press 99 now **loses** to a possession side (−10), a high line (−12) and direct play (−6), and wins
  only +3 against a low block (F5.2 +40). It is still the best answer to a counter-attacking side (+25) — "high pressing may still be the strongest choice in some situations".
* **Preset integration.** `every_ofm_match_is_played_with_the_f53_football`, `the_instant_path_plays_the_f53_football`, `an_interactive_match_is_started_with_the_f53_football`.
* **Geometry.** `a_high_press_stretches_the_team_between_its_lines`, `a_low_press_drops_by_less_than_a_high_press_steps_up` (killed by symmetric stretch),
  `a_low_press_still_closes_the_carrier_down_in_its_own_box` (killed by removing the floor).

## 11. F5.2 realism regression

`F52_FLAGS=f53 scripts/f52-m12-realism-gate.sh <world> 40 12` (the F5.2 gate, 480 matches, real career inputs), three worlds, 7 (used to tune F5.2), 11 and 13 (held out):

| world | goals a match F5.2 → F5.3 | shots a match | on target | dismissals | penalties | ball dead | outfield km / 90 | gate |
|---|---|---|---|---|---|---|---|---|
| 7 | 2.95 → 2.84 | 24.7 → 24.3 | 49 % | 0.113 | 0.29 | 30 % | 11.9 | PASS |
| 11 | 2.97 → 3.00 | 30.7 → 28.7 | 49 % | 0.104 | 0.24 | 29 % | 11.6 | PASS |
| 13 | 3.02 → 2.82 | 27.9 → 26.6 | 49 % | 0.096 | 0.21 | 29 % | 12.0 | PASS |

(F5.2 columns: world 7 from the 8-seed panel, world 13 from the same script with `f52`, world 11 from the 8-seed panel.) Goals move by −0.2 … +0.03 and shots by −0.4 … −2.0: **inside the F5.2
envelope (2.89–3.02 goals, 24.6–28.4 shots) on world 11 and 0.05–0.2 goals below it on worlds 7 and 13.** The shave comes from the box floor (a second defender closing the carrier near goal);
a smaller zone and a larger stretch were chosen to give back most of it and no event-rate system was touched to recover the rest. Neutral-tactic matches are bit-identical to F5.2.

## 12. Tests

| Suite | Result |
|---|---|
| M12 `cargo test --release --no-fail-fast` at the committed `28f34b1` content | **640 passed, 0 failed, 29 ignored** (F5.2's report: 631 at `ded5a8a`, the full suite not re-run on its last commit; +7 new in `tests/f53_press_costs.rs`) |
| OFM `cargo test --workspace --no-fail-fast` at the final code | **4 596 passed, 0 failed, 73 ignored** (F5.2: 4 580 passed, 1 failed — its documented `dormant_leagues…` bound was already moved) |
| `ofm_core --lib` | inside the workspace total; includes the new economy tests (§6) and the engine-pin tests (1 976 passed before the last tests were added) |
| Clippy `--workspace --all-targets --locked -D warnings`, default and `--features mcp` | clean; `cargo fmt --all` clean in both repositories |
| `season-harness --test f53_financial_stability` | pass (45 s release); fails when the AI ceiling is disabled (§6) |

**Not run:** frontend tests, `npm run build`, Biome (no frontend file changed); `--features mcp` *tests* (the diff touches no MCP tool; clippy with the feature passes); the project's read-only
reviewer agents (`ofm-architecture-reviewer`, `ofm-test-reviewer`, `ofm-dedup-reviewer`) were **not** run — this session spawned no agents; `i18n-auditor` and `ui-accessibility-reviewer`
do not apply (no player-visible string or UI file changed). A manual self-review against the code-quality rules found no function over its ceiling (`renew` 65 lines, `best_signing` 68), no
`unwrap` or `#[allow]` added, no dead code (the compiler would say), and the instrument is the only large addition (`finance_study.rs`, ~420 lines, outside every save).
Tests written after the code and not seen failing first: the guard tests (`ai_finance_guard`) were edited from the F5.2 grace test; their discrimination was shown by the mutants of §6 rather than by a red run.

## 13. Performance

The full-world study costs 22–25 minutes a seed single-threaded (1,297 s measured for F5.2's code; three seeds ran side by side). Validation cost was cut without changing what is simulated:
the instrument reads the cash journal that already exists and keeps its own per-season rows (not stored in saves), the diagnostic `lower_nations` world (80 clubs, 75 s) reproduces the
shipped world's debt and is used for iteration only — **never as an acceptance world**; every acceptance figure above is the 440-club world. The decisions added cost one extra `calc_wages` per recruit
at the season end and nothing per day.

## 14. Remaining defects inside the two authorised scopes

1. **A low line is still the weakest tactic.** Low block −21 and counter −27 against neutral (F5.2 −35 / −39); the press-15 half is fixed (+1), the *line* is not: line 20 alone concedes more shots than a balanced
   line. No change was made to the defensive line in this run (scope: pressing).
2. **The press's benefit is flat and the sliders around it were not re-tuned.** Press 99 is −6 against neutral and best only against a counter-attack; press 70 (+5) is now the best press setting. Width and tempo have the
   same "no physical cost" issue F5.2 named (not touched; out of scope).
3. **Strength by standing is flatter.** Founding top quarter 0.74 → 0.61–0.66, bottom 0.29 → 0.37–0.39 (§5). Financial structure is intact; whether the sporting gap should be wider is a design question
   (a rich club can pay up to its budget but only buys what the market offers; stars are not bought out of poor clubs: release clauses are out of scope).
4. **Cash accumulates** (median 66 M after thirty years; the rich quartile 167 M): clubs have no outlet beyond wages and fees. Harmless to solvency; it is what the budget rule leaves unspent.
5. **Side effects on systems this run was not to touch, measured, not tuned:** free agents 3,604 → ~1,780 at season 30 (population unchanged at ~14.8 k), free signings and releases about
   +50 % a world-life (§5), and the mean age of players at clubs 21.7 → 23.3 (fewer youths are on wages as large as the first team's, and the free-agent glut is gone). Neither the free-agent
   population nor the age distribution was a target; both are reported so that the next run can judge them.
6. **Three seeds is a sample.** All three agree to within 1 % on every figure, but they share a world generator; a different nation mix could behave differently.
7. **No GUI playthrough** (unchanged from F5.2): the economy was driven by the same `process_day` / `advance_to_next_season` the game calls, through the harness, with the user's club held in place.

## 15. Commits and pins

| Repository | Commit | |
|---|---|---|
| M12 (`f5.3/pressing`) | `28f34b10d4fe7cadacc0030acaaa0cd1aa9fdf5f` | `feat(f5.3): full_match_f53 - what a press costs and what it leaves behind` — the revision OFM pins; the pin was checked against GitHub with the local override moved aside |
| OFM (`claude/run-f5-3-finances-and-pressing`) | `d9d162de` | `fix(economy): an AI club's wage decisions stop at its budget, and its money buys quality` |
| OFM | `df1f9c5f` | `feat(engine): OFM plays every new match with the M12 F5.3 football` |
| OFM | the report commit (`git log -1` on the branch) | this document |

## 16. Push status

Both branches are pushed; **nothing is merged** into `apfm/main`, `develop` or any other branch. M12's uncommitted licence change and the untracked `LICENSE`/`NOTICE` were left as found and never staged; the
viewer was not touched. No credential was read or written; user saves and original datasets were not touched.

## 17. GO / NO-GO

* **Objective A — economy: GO.** 0 of 439 clubs in debt at every checkpoint (seasons 5 … 30) on three seeds of the shipped 440-club world where F5.2 had 170–176 at season 30; wage bills over 110 % of budget 371 → 1–2;
  mean operating result negative → positive; cash P10 +22 M (was −20 M); squads healthy (thin groups 107 → 1–3); the four spending paths now answer to one ceiling and the tests and mutants say so. Caveat §14.3–5.
* **Objective B — pressing: GO.** Press 99 −6 against a neutral side (F5.2 +8, and +26 in F5.2's own grid), no longer dominant against low block / possession / direct / high line, still best against a counter,
  paid for in fatigue (0.59 against 0.38) and exposure; a weaker side maximising the press wins 6 %; the F5.2 gate passes on three worlds. Caveat §14.1–2: the low line remains the weakest tactic.
* **Beta: NO-GO, unchanged from F5.2** — no GUI playthrough, ageing unchanged, shots on target / corners / fouls are outside this run.
