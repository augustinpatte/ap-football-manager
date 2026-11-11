# APFM P4.7: strategic AI completion

Branch `claude/apfm-playable-career`, from `b5430826` (P4.6). Not merged into `apfm/main`, not pushed. M12, the external 2D
viewer and every frontend file were left alone. The long-career evidence is in `docs/APFM_P47_LONG_CAREER_VALIDATION.md`.

Evidence labels: **RUN** (executed this phase), **READ** (read in source), **REVIEW** (a reviewer agent's finding, checked here).
"Baseline" is `b5430826` built in its own worktree with its own target directory, played with the same seeds. "Final" is
`6667ef60`. Gate world: 80 clubs, ~3,740 players. Ten seasons × three seeds (1, 2, 3) is 2,370 club-seasons.

## 1. Verdict in one table

| Slice | What was asked | What happened |
| --- | --- | --- |
| 1. Adaptive formations | Every AI club kept one formation for 30 seasons | **Done.** 5-7 formations in use, top share 57 % → 42-44 %, about 0.2 changes per club-season after the first season, no formation wins beyond its strength |
| 2. Youth pathway | Ready prospects had no route to minutes | **Partly.** The route exists and is used (academy players as good as a starter: 56 → 31 at season 10); the share of prospects under 450 minutes and the cohort size did **not** move |
| 3. Loans | Idle loans rose from 21 % to 41 % over ten seasons | **Improved, and narrowing late.** 12 → 26 % idle over ten seasons for the same minutes in a quarter fewer loans; over seasons 11-30 (paired 30-season runs) 51 → 33 % (for minutes) and 49 → 35 % (others), but at season 30 the ordinary-loan rate is back to 57 % against 59 % |
| 4. Sales and renewals | Strategy did not reach them | **Connected, effect not shown.** Renewals and the funding-sale rule read the club profile; ten-season squad quality, finances and utilisation are indistinguishable from slice 3 |
| 5. Appointments | Reputation only | **Done.** Misfit of the manager appointed 0.29 → 0.14; tenures not improved (median 187 → 167 days) |
| 6A. Opening-rating pricing | Wide players 20-25 % cheap | **Measured, deliberately not changed.** 29 % of permanent deals for 19.5 % of the players; the gaps close by themselves (wage 0.88 → 0.91, value 0.85 → 0.92 over ten seasons) |
| 6B. Pending wage commitments | Delegation did not reserve agreed deals | **Fixed**, isolated, four of five tests red without it |
| Cost | Preserve F5.5 | **+4.6 % instructions over ten seasons, +2.3 % over thirty**; peak memory lower (190 MB against 196 MB; 661 MB against 707 MB over thirty seasons) |
| Verdict | | **CONDITIONAL GO** for P5, conditions in section 13 |

## 2. Commits (oldest first, on `b5430826`)

| Commit | Change |
| --- | --- |
| `8c11123d` | `feat(ai-formation)`: AI clubs choose and change their formation; two ignored studies |
| `05dede80` | `perf(ai-formation)`: neutral horizon, two looks at a new manager's inheritance (+4.3 % → +1.8 %) |
| `f8e51834` | `feat(youth)`: `youth_pathway.rs`, promotion on merit scaled by manager trust; deputy choice tilted by trust |
| `6e37fb8b` | `fix(loans)`: `LOAN_MIN_GAIN`, loan checks read the memoised marginal |
| `335625e6` | `feat(strategy)`: club profile reaches renewals and the sale that funds another player |
| `04371d6f` | `feat(appointments)`: identity fit among managers of similar stature |
| `70d90026` | `fix(delegation)`: reserve the wages of deals agreed and waiting to register |
| `1e8ed32d` | `test(p47)`: pricing, appointments and strategy-read cost studies |
| `731c0159` | `fix(p47)`: act on the architecture and dedup reviewers (one change reverted below) |
| `c56ecbf9` | `perf(appointments)`: read strategy once a day and only when a club has someone to choose from |
| `f65ba133` | `test(p47)`: act on the test reviewer |
| `6667ef60` | `fix(p47)`: keep the saved eleven a formation change writes as it was (the revert, section 10) |
| (this report) | `docs(p47)` |

## 3. Architecture changes

No crate boundary moved, no `Cargo.toml` changed, nothing under `domain`, `db` or `engine` changed, **no serialized field, no
migration, no SQL**. Everything new is a pure function of persisted state.

| Where | Change |
| --- | --- |
| `ofm_core/src/ai_formation.rs` (new) | Scores each of the eight formations (`player_rating::FORMATIONS`, the same eight the tactics screen lists) with the squad model the market prices signings by, at the neutral horizon, absences allowed for; adds a small taste (club play style, the manager's risk, a personal liking hashed from his id); moves only when the best beats the current by a margin |
| `ai_tactics.rs` | The weekly review calls it; `FormReading` gained points a game over eight results |
| `youth_pathway.rs` (new) | An academy player the club's planning model says improves its side (starter, cover, succession from 18) is promoted; bar scaled by the manager's youth trust; at most two a review, one look a month |
| `turn/squad.rs` | `ai_select_starting_xi_by` gained `youth_trust`; the deputy choice is `fresh_deputy` (function 147 → 129 lines, shorter than before the phase) |
| `transfers/buyer_view.rs` | `LOAN_MIN_GAIN = 2 × MIN_GAIN`; the loan checks read the memoised marginal; `ClubSquad::present_gain` and `starts_today` removed (dead) |
| `ai_contracts.rs`, `ai_squad_sales.rs`, `transfers/strategy.rs` | `SquadFacts` carries a `ClubProfile`; three renewal rules and `reinvestment_demand`; `profiles_among` / `strategies_of` / `profile_in` read the day's reviewing clubs once, at the start of the day, in `turn/mod.rs` |
| `ai_manager_appointments.rs` | `candidates`, `mismatch`, `FIT_WEIGHT`; `appointment_mismatch` is a public diagnostic |
| `transfers/registration.rs`, `staff_system/delegation.rs` | `pending_wage_commitments`, subtracted from the wage room in `books_of` |

### 3.1 The design in one paragraph per decision

**Formation.** Three different things decide a shape and they are kept apart. The club's formation is the persisted
`Team::formation`, with inertia (a margin). The manager's taste is derived from his id (`ManagerIdentity` risk and a personal
liking) and travels with him; a new manager looks at the shape he inherited on his first review and again a month in, with a
margin that starts 60 % lower and fades over 90 days. The club's philosophy is its play style. Poor form lowers the margin for
an adaptable manager and never to zero, and a winning run raises it, so **a defeat is not a reason; a better shape is**. A
temporary match-day adjustment for injuries is not implemented (section 11).

**Youth.** A prospect the model does not want stays in the academy and keeps developing; one it wants is a senior, where the
picker plays him if he is the best player for the slot. No quota and no minutes target. Trust moves the bar (half of the
transfer floor at full trust, one and a half times at none) and, when a manager rests a tired starter, which adequate deputy he
gives the shirt to (at most five rating points, never an inadequate one).

**Loans.** The model's own gain predicts minutes (section 5.3), so a loan has to add twice the transfer floor.

**Sales and renewals.** A club built around its young players renews a prospect whose ceiling is up to eight points under the
squad's median; a financially constrained club does not renew a player at the standard whom the side would not miss; a
contender renews a veteran two years past his group's peak and asks half as much again of a sale that funds a replacement; a
small developing club a quarter less. A neutral club and the manager's assistant decide exactly as before.

**Appointments.** Stature first (the reputation gap over 150), fit second at half weight. A club may deliberately appoint a
manager of another school: a candidate 75 reputation points closer outweighs the worst fit.

**Wage reservation.** The room the assistant judges a signing against has the weekly wages of the club's agreed purchases (at
the wage their terms name, or the standard one) and loans (at their share) taken off it. It is derived from the offers, which
are saved with the players, so it survives a save, ends with the agreement and is never counted twice once the deal registers.
No agreed deal is cancelled: a second approach becomes a proposal for the manager.

## 4. Slice 1: formations

### 4.1 Root cause

No code path in the backend ever wrote `Team::formation` for a club the player does not manage (**READ**: `Team::new`
defaults to 4-4-2, nothing else assigns it). So the P4.5 "100 % one formation for 30 seasons" was not a decision at all.

### 4.2 What the engine rewards (a controlled experiment, **RUN**)

Reviews frozen, every AI club given a formation by a hash of its id whatever its squad; 891 club-seasons, gate, seeds 1-3, three
seasons. Within-league regression of points a game on the strength of the best fourteen:

| Finding | Value |
| --- | --- |
| Beyond strength and squad score, by formation | within ±0.05 points a game (standard error ±0.023): **no formation is universally better**; 4-2-3-1 +0.053 is the largest, about 2σ of eight comparisons |
| Points a game per point of squad-model deficit from the club's best formation | **−0.005**: a club in a shape the squad model values a whole rating point lower loses nothing visible |

The second row is a finding for P5, not for this phase: **the engine barely prices formation suitability**, so the squad
model's formation score is not validated by results. Choosing a shape for its model score is "sporting" in the model's terms.

### 4.3 The squad model has a systematic favourite

At the opening, 4-3-3 is the model's best formation for about 60 % of clubs. The cause is the world's squads, not a scale error
(**READ**): after the D6 rating correction wingers and attacking midfielders are rated far higher than the wide midfielders a
4-4-2 asks for, and 4-3-3 uses the wingers. Hence 4-3-3 stays the commonest shape. It is not "optimal" (previous section); it
is where the squads are.

### 4.4 Results (**RUN**, 3 gate seeds × 10 seasons, 2,370 club-seasons)

| Measure | Baseline | Final |
| --- | --- | --- |
| Distinct formations in use, season 1 / 5 / 10 | 1 / 1 / 1 | 6 / 4-7 / 5-7 |
| Largest share, season 1 → 10 | 100 % | 57 % → 42-44 % |
| Changes per club-season, all seasons / after season 1 | 0 | 0.27 / about 0.2 |
| Clubs changing more than twice in a season | 0 | 1-4 a season (of 79) |
| Flip-flops (back in the shape it left within 60 days) | 0 | 60 of 641 changes (9.3 %) |
| Changes within 35 days of a new manager | n/a | 63 % (season 1: 95 %, because every opening manager is "new") |
| Squad-model score of the shape played, against the best offered | n/a | 0.01-0.04 rating points short (suitability) |
| XI fit, season 1 / 10 | 68.13 / 70.60 | 69.13 / 71.50 |
| Clubs with a weak starting slot, season 1 / 5 / 10 | 19.7 / 11.0 / 1.7 % | 8.7 / 4.3 / 0.3 % |
| Points a game beyond strength, by formation (organic) | n/a | 3-5-2 +0.082 (±0.034, n 59); 5-3-2 +0.052 (±0.011); 4-3-3 −0.024 (±0.007); others inside ±0.03 |

The organic 3-5-2 and 5-3-2 residuals are **selection**, not engine reward: the controlled experiment, with no selection, gave
+0.015 and −0.016. Clubs that choose them have squads (strong centre backs, wing backs) that strength-of-the-best-fourteen
understates.

The XI-fit and weak-slot rows are a real improvement: clubs now play a shape their squad fits. They move at season 1, on
nothing the market did.

**Statistical meaning.** The formation changes are deterministic outcomes of the squads, so a p-value would be false precision;
what can be said is that 2,370 club-seasons on three seeds show the pattern in every season of every seed.

### 4.5 Performance cost of slice 1

+1.8 % instructions after the cadence fix (the first cut was +4.3 %: it read every club's strategy on nearly every day, and
reviewed a new manager's shape four weeks running).

## 5. Slice 2 and 3: youth and loans

### 5.1 Why the prospect cohort shrinks (**RUN**, `p47_youth_causes`, summed over three seeds, ten seasons)

| Hypothesis | Verdict |
| --- | --- |
| The cohort halves because intake is too small | **Partly a transient.** The opening world holds 2,182 prospects (9 a club) against a steady state of about 1.7 intake a club a year, so the stock falls towards roughly 1,200 (5 a club) and flattens (1,438 → 1,302 → 1,245 → 1,192 over seasons 7-10 in the baseline). New-to-world players fall 700 → 400 a season as academies fill (intake is 1-3 a club, set by what the academy lacks) |
| Players age out of the cohort | Large early (172 → 338), negligible after season 6 |
| Players mature (under six points of room left) | **The biggest exit**: 300-540 a season |
| Released, retired, sold to the user | Small (30-76 a season released from season 4) |
| Academy production is weak | Not tested separately; intake is a designed 1-3 a club, **not changed** |
| Lack of minutes | **Yes, for the academy**: academy-role prospects (20-45 % of the cohort) play 0-2 minutes a season, 100 % under 450, because the match-day pool is the senior squad |
| Excessive squad competition | A prospect who is not ready plays little, rightly: 90 % of those below the bar play under 450 |

The shipped-world figures (cohort 4,328 → 2,202) are not reproduced here (section 6 of the validation report).

### 5.2 What the study found and what was done

Position-aware readiness (as good as the club's last starter in his group, within two points):

| | Baseline | Final |
| --- | --- | --- |
| Academy-role prospects who are first-eleven ready, season 1 / 5 / 10 | 26 / 32 / 56 | 6 / 9 / 31 |
| Their minutes | 3-64 a season, 96-100 % under 450 | 0-4 |
| Senior-role first-eleven-ready prospects, season 10 | 157 (25 % under 450) | 199 (30 %) |
| Prospect cohort at season 10 (three seeds) | 1,205 | 1,195 (**no change**; an intermediate run read 1,320, so ±10 % is noise) |
| Career-study cohort: minutes at season 10, share under 450 | 351, 85 % | 408, 85 % (**no change in the share**) |

So the specific defect (ready players with no route) is much smaller (−45 % at season 10, −80 % at season 1) but is not gone: a
player who is ready by the group proxy is often fifth-choice in a specific slot, and the model rightly keeps him in the
academy. The aggregate prospect measures the brief names (cohort, share under 450) **did not move**, and this report does not
claim they did. Growth is unchanged (+5.15 baseline, +5.07 final).

### 5.3 Why loans went idle (**RUN**, `p47_loans`, 60+ day loans)

Of loans that arrived behind the borrower's starters (by rating rank in the position group), 43-57 % went idle in the baseline; of those that
arrived as starters, 1-9 % (ordinary loans) or about 8-14 % (loans for minutes). The share of loans arriving behind the
starters rises over a career (47 % → 74 % of ordinary loans). Tested and **rejected**: injuries (today's squad and the planning
squad gave identical gain and start for every accepted loan; no idle loan was one where the loanee was injured). Not the
cause on its own: a better player joining the borrower (it co-occurred with a third to a half of idle loans, all of which were
already behind the starters at the start).

The model's own gain predicts minutes (one seed, 8 seasons, 466 loans):

| Predicted this-season gain (rating points over the eleven) | Idle, loans for minutes | Idle, other loans |
| --- | --- | --- |
| under 0.2 | 36 % | 41 % |
| 0.2-0.4 | 29 % | 35 % |
| 0.4-0.6 | 15 % | 19 % |
| 0.8 and over | 0 % | 0 % |

The floor under a loan was the transfer floor (0.12), so most accepted loans were marginal starters a manager's selection noise
(misjudgement up to nine points, rest days) leaves out. At twice the floor:

| Loan start season | 1-3 | 4-6 | 7-9 | 10 |
| --- | --- | --- | --- | --- |
| Loans (baseline → final) | 305 → 223 | 620 → 456 | 529 → 404 | 318 → 178 |
| Idle share of 60+ day loans | 21 → 12 % | 30 → 16 % | 40 → 22 % | 41 → 26 % |
| Minutes per loan day | 8.64 → 10.13 | 7.31 → 9.81 | 5.97 → 8.57 | 5.66 → 8.04 |

Total minutes played on loan: 12,125 → 11,625 units (−4 %) in 27 % fewer loans. At three times the floor the total falls 10 %,
which is why the constant is 2×. Idleness still **rises** over a career (12 % → 26 %): the residual cause is still "behind the
starters on arrival" (36 of 45 idle loans in season 10), and this report does not claim the long-career loan problem is solved.
Season 10 and later are included in the table. **Over thirty seasons (paired, same seeds, rebuilt baseline):** pooled over
seasons 11-30, long loans that went idle fell from 293 of 574 (51 %) to 108 of 325 (33 %) for loans for minutes, and from 1,524
of 3,095 (49 %) to 592 of 1,700 (35 %) for the others, in 45 % fewer loans. But the gain narrows with the career: for ordinary
loans the idle share at season 30 is 57 % against the baseline's 59 %, and for loans for minutes 44 % against 73 %. The
structural cause (a loan arrives behind the borrower's starters, because mature squads need depth and not starters) is not
removed by a higher bar, only thinned. Details in the validation report.

The loan check now reads the buyer view's memoised marginal instead of a fresh model evaluation per target per day, which
saved the cost the bar would otherwise have added.

## 6. Slice 4: strategy in sales and renewals

The wiring is tested (`the_days_strategy_reading_reaches_the_clubs_renewals`, three renewal rules each red when removed). The
**effect is not demonstrable**:

| Ten seasons, three gate seeds | Slice 3 | Slice 4 |
| --- | --- | --- |
| XI fit season 10 | 71.53 | 71.43 |
| Clubs with a weak slot, season 5 | 4.3 % | 4.3 % |
| Unused joiners, season 10 | 18.3 % | 19.0 % |
| Joiners per club, season 5 / 10 | 3.63 / 2.67 | 3.17 / 2.13 |
| Transfer fees per club, season 5 / 10 (M) | 1.30 / 1.56 | 1.09 / 1.16 |
| Clubs in debt, club-seasons over 110 % of wage budget | 0, 0 | 0, 1 |

Squad quality, finances and utilisation are indistinguishable. Turnover is lower from season 5 (joiners −13 % and −20 %, fees
−16 % and −26 %): a plausible consequence of contenders holding on and constrained clubs not renewing, but three seeds cannot
say whether it is the rule or the draw. **Cost: +2.9 % instructions** (one 0.25 ms world-wide strategy read a day, restricted to
the reviewing clubs). The brief's examples (a constrained club selling an expensive player when a replacement exists; a rebuilding
club trading short-term strength for opportunities; an ageing squad planning succession rather than losing several starters)
are served by the rules above and by the existing succession planner, and none of them is measurably more frequent.

**Not done:** the listing side of the wiring (`reinvestment_demand` reaching `ranked_sellable_for`) has only an ordering test;
a fixture with a reinvestment gain between 2.25 and 4.5 times the net loss was not constructed. The "sell" rule for a player's
contract length and wage demand beyond what the renewal already reads was not extended.

## 7. Slice 5: appointments (**RUN**, 1,018 appointments, 3 gate seeds × 10 seasons)

| | Stature only | Stature plus fit |
| --- | --- | --- |
| Misfit of the manager appointed (0 perfect, 1 opposite) | 0.289 | **0.142** |
| Mean misfit of the unemployed pool at the time | 0.312 | 0.325 |
| Difference (standard error) | −0.023 (0.005) | **−0.183 (0.004)** |
| Completed tenures | 1,007 | 1,053 |
| Tenure, mean / median (days) | 395 / 187 | 380 / 167 |

Fit has real selection power. Tenure is **not** improved and is slightly shorter; the sample (three seeds) cannot separate that
from noise. Managers leaving a season: 1,080 (baseline) against 1,053 (final) across the 30 seed-seasons, so turnover has not
risen. What this does not show: that a better-fitting manager does better; no result measure is attached to the appointment.

## 8. Slice 6: financial integrity

### 8.1 Pending wage commitments (confirmed, fixed)

**Exact path (READ):** `delegation::books_of` set `wage_room = wage_budget − calc_wages`, and a deal agreed while the window is
shut is not on the club's books until it registers, so two approaches judged against the same room could both be agreed (the P4.6
probe: room 28,120, approaches at 18,000 and 16,000, both registered, +1.8 % over budget). `TransferOffer.wage_offered` is written
as 0 by every production path, so the agreed wage is read from the offer's terms or the buyer's standard terms
(`buyers_agreed_terms`).

**Tests (RUN, all in `staff_system/delegation.rs`):** two pending deals; a deal that falls through (withdrawn, rejected); a deal
that registers (counted once); a loan at its share and another club's deal reserving nothing; a deal on standard terms; and a
save/load through JSON. **Red-first:** with only the subtraction removed, four of the five fail (the "registers once" test is a
no-double-count guard and passes either way, as it should). The season-harness regression that exposed it
(`total_delegation_manages_multiple_transfers_within_live_finances`) passes. **Concurrency:** the value is computed from `&Game`
inside the one `update_game` lock, so there is no separate state to race; no concurrent test was written.

**Remaining:** the same reservation is **not** applied to the player's own manual bids (`bids.rs` builds its bill from
`calc_wages`) or to the AI buyers' `SigningBooks` (their deals register on the day they are agreed, so the window is closed to
the gap there, but this was not proved). Pending **fees** have the same gap for cash (a registration then fails with
`InsufficientFunds`, which is notified). Neither was changed.

### 8.2 Opening-rating pricing (measured, deliberately not changed; `p47_pricing`, 3 seeds × 10 seasons, 1,696 deals)

| | Five positions (wingers, wide, defensive and attacking midfielders) | Others |
| --- | --- | --- |
| Share of rostered senior outfielders | 19.5 % | |
| Share of completed permanent deals | **29.3 %** (497) | 70.7 % |
| Fee as a share of market value | 0.820 | 0.820 |
| Weekly wage per rating point, opening → end | 105.7 → 142.2 | 120.5 → 156.5 (ratio **0.878 → 0.909**) |
| Market value per rating point squared, ratio | opening **0.854** → end **0.916** | |

**Does it create systematic bargains?** A modest one: the five positions are about 12 % cheaper in wages and 15 % in value per
rating point at the opening, and clubs buy them 1.5 times as often as their share of the players, at a fee that is the same
share of value (the discount is in the value itself). **Does it destabilise?** Not visibly: no club in debt, none over 110 % of
its wage budget in seasons-end rows, and the gaps **close by themselves** as players are renewed (`quality_wage_catch_up`
already re-prices at renewal) and trained. **Unfair advantage between clubs?** All AI clubs read the same prices; a club that
happens to need wide players gains, which is a squad-composition effect (and part of why 4-3-3 is common, section 4.3).
**Recommendation:** no re-pricing now. If an economy balance pass is wanted, price value and wage from the corrected rating in one
dedicated change calibrated against `APFM_ECONOMY_BALANCE_REPORT.md`; the owner decision from P4.6 stands.

## 9. Runtime and memory (**RUN**, instructions retired as the primary figure)

Gate, 3 seeds × 10 seasons, invariants off, `/usr/bin/time -l`, release profile, baseline in its own worktree:

| Tree | Instructions | Peak RSS |
| --- | --- | --- |
| Baseline `b5430826` | 2,436.2 B | 195.5 MB |
| Slice 1 (after cadence fix) | 2,479.5 B (+1.8 %) | 194.2 MB |
| Slice 2 (after cadence and screen) | 2,524.2 B (+3.6 %) | 196.8 MB |
| Slice 3 (loan bar and memoised marginal) | 2,461.7 B (+1.0 %) | 199.5 MB |
| Slice 4 (strategy, shared read) | 2,533.5 B (+4.0 %) | 195.5 MB |
| Final `6667ef60` | **2,547.5 B (+4.6 %)** | **190.3 MB (−2.7 %)** |

The F5.5 saving (−26.9 % against F5.4) is preserved: the final tree is 4.6 % above the pre-phase tree and still about 24 %
below F5.4. Wall time is not a result (other work shared the machine). The cost is spread: formations 1.8 %, academy
promotion 1.8 %, the strategy read 2.9 %, less 2.6 % the loan change saved. Each was measured twice and tightened once.

## 10. Reviewers

| Reviewer | Outcome |
| --- | --- |
| `ofm-architecture-reviewer` | **1 blocking, fixed**: `ai_select_starting_xi_by` grew to 147 lines (deputy choice extracted, now 129, below the size before this phase; it is still over the 100-line ceiling, as it was). **Fixed:** the freeze of formation reviews is a drop guard; the day's strategy read is taken at the start of the day (it depended on whether an unrelated review had run). **Listed, not blocking:** public study surface in `ofm_core` (`read_all_clubs`, `appointment_mismatch`, `FORMATIONS`); `#[cfg(test)]`-only wrappers in `ai_contracts.rs`; `profile_of` per vacant club (now read once). **Not fixed, with reason:** `adopt` writes a saved eleven from every healthy player of the club, an academy player included. Narrowing it to the match-day pool was made and **reverted**: it moved the generated world onto a latent gap that `season-harness/tests/delegated_career.rs` asserts against (a starter who retires leaves the user's saved eleven at ten until the next weekly lineup run, and the test asserts eleven after every day). The AI re-picks and re-saves its eleven at every match, so what a formation change writes is transient; the gap is recorded as open |
| `ofm-dedup-reviewer` | **No blocking.** Listed for epic #589: the formation list exists four times (Rust `FORMATIONS`, `spatial_adapter` arms and test list, the tactics screen; a test now cross-checks `FORMATIONS` against `formation_geometry`); a week-modulo-hash cadence helper written twice (`is_review_week`, `is_academy_week`); `tenure_days` against `days_covered`; a 3/1/0 points mapping now in three places; `(x − 0.5) × 2` written five times; `group_of` three times in the studies (`squad_floor::group_index` is `pub(crate)`); within-league regression helpers repeated across studies; the wage reservation is wired into delegation only |
| `ofm-test-reviewer` | **4 blocking, fixed and mutation-checked:** the strategy reaching the day's renewals; the results-to-margin path (win 3, draw 1, loss 0 over eight, tenure and form reaching the review); the player's club genuinely due the same day as the club that is moved (the control was vacuous); the monthly academy gate. **Non-blocking, fixed:** a deal on standard terms reserves the standard wage; trust never fields an inadequate deputy; a doc/fixture mismatch. **Still open:** the listing side of the strategy wiring (section 6); a tie-break test that cannot reach the tie; the `SCREEN_MARGIN`, `most_ready` tie-break and `Succession` pathway are not exercised through `promote_ready_academy_players`; `profile_in` has no equivalence test against `profile_of`; the harness studies print standard errors that ignore repeated clubs and must not be quoted as confidence intervals |

## 11. What is fully implemented, and what remains

**Fully implemented:** the formation review (A and B of the brief's three levels); promotion on merit with manager trust; the
loan bar; strategy in renewals and the funding sale; identity fit in appointments; the wage reservation for delegated recruitment.

**Not implemented or not shown:**

1. **Temporary match-day adjustment (C).** A club does not change shape for the injuries and bans of a day. The live engine AI
   still has its in-match `bolder_formation`. This is the part of §1 the phase did not reach.
2. **The prospect-share goal.** The cohort and the share of prospects under 450 minutes did not move; academy production
   (intake 1-3 a club) was left alone as designed. The brief's "retained, loaned, promoted, sold or released" decision is
   covered for promotion and loan; sale and release of a prospect still follow the existing contract and listing rules.
3. **Sales and renewals effect on squad quality, finances and utilisation.** Wired and tested, not measurably better.
4. **Appointment outcomes.** Fit was optimised; no result or tenure benefit is shown.
5. **Pending fees and the player's own bids** are not reserved (section 8.1).
6. **Opening-rating pricing** was measured and not changed, as a decision.
7. **No paired shipped-world baseline.** The 30-season shipped run (440 clubs) is in the validation report with its own invariants and finance checks; the baseline was not re-run on that world (another half hour), so its cohort, minutes and loan figures are compared with the P4.5 figures, which predate D6 and are not paired.
8. **Frontend:** no file changed; no screen shows an AI club's formation change (no news item either). **The standing GUI
   condition is unchanged: no person has played the window.**

## 12. Tests (**RUN**)

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, and with `--features mcp` | clean |
| `cargo test --locked --workspace --no-fail-fast` | **140 targets, 4,856 passed, 0 failed, 94 ignored** (P4.6: 132 / 4,810 / 86; the 8 new ignored are the studies) |
| `cargo test --locked --lib --features mcp` | 653 passed, 0 failed, 8 ignored |
| `npm test` | 340 files passed, 1 skipped; **2,993 tests passed**, 4 skipped (unchanged; no frontend file was touched) |
| `npm run build`, `npm run lint`, `npm run format:check` | all exit 0 |

One workspace run **failed** during the phase and is explained rather than hidden: `delegated_career` went red on `731c0159`
(section 10), found by bisecting the 11 commits in a worktree and reverted in `6667ef60`. Not run: `cargo deny`, `cargo machete`,
`npm run knip`, `npm run audit:i18n`, and the `/preflight` skill as one invocation. No player-visible string was added (no news
item, no message), so the i18n reviewer and the accessibility reviewer were not required.

**Mutations run** (remove only the change, see red, restore, see green): the formation `adopt` call; the defeat margin
(`ADAPTABILITY_DISCOUNT`); the academy gate and its wiring; the trust bar; the deputy lean and its wiring; `LOAN_MIN_GAIN`; the
three renewal rules; the day's strategy reading; `FIT_WEIGHT` both ways; the wage subtraction; the user filter in `ai_clubs`;
points per draw; tenure and form reaching the formation review.

## 13. Decision

### CONDITIONAL GO for P5, Match Engine and Sporting Realism Validation

**Why go:** AI clubs now choose and change shape for reasons the squad gives, in seasons the whole world runs deterministically,
reloads identically (three seeds, ten seasons, 60 days on), and keeps its invariants clean; loans are half as idle at the same
minutes; ready academy players have a route; appointments consider fit; a latent finance gap is closed. No club went into
debt in any run. The cost is +4.6 % instructions and no extra memory.

**Why conditional:** the part of this phase that needs the engine is the part P5 owns, and three things are not as good as the
brief hoped.

1. **P5 must treat these as engine questions.** The engine barely prices formation (−0.005 points a game per rating point of
   squad deficit), so formation choice in this phase is sporting only by the squad model's definition; and 4-3-3 is the
   commonest shape because the corrected wingers are the world's best players. Both are P5's to validate.
2. The prospect measures (cohort, share under 450 minutes) did not improve and the strategy wiring has no measured effect:
   neither should be presented as solved.
3. Fix or consciously accept before shipping: the delegated-career latent gap (section 10), the missing match-day formation
   adjustment, and the unreserved fees and manual bids (section 8.1).
4. The standing GUI condition from P2.5 and P3 is unchanged: no person has played the window.

P5 was not started.
