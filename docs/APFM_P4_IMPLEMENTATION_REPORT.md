# APFM P4 implementation report: strategic football intelligence

Branch `claude/apfm-playable-career`, from `e5282b67`. Not merged into `apfm/main`, not pushed. M12 and the
external 2D viewer were not touched, and no frontend file changed.

Companion documents: `APFM_P4_STRATEGIC_ARCHITECTURE.md` (the audit and the design) and
`APFM_P4_LONG_CAREER_VALIDATION.md` (the evidence and its limits). This report says what was built and
whether it can be trusted. **Read the validation document's first section before relying on any number
here: the longest run was three seasons, and the shipped world was run for one.**

## 1. Verdict

P4 found and fixed a real inconsistency at the centre of the AI (the squad model priced signings for a side
the manager did not field), gave managers an identity, and made prospect loans purposeful. It did **not**
build a long-run strategic AI that is validated over a career, and it did not touch finance, formations,
sales or appointments. The measured sporting effect of the market changes is small; the measured
consistency and loan effects are large. Phases B, E and F are partly done; they are listed honestly in §5.

## 2. Exact commits

All on `claude/apfm-playable-career`, oldest first, on top of `e5282b67`.

| Commit | Change |
| --- | --- |
| `5b16d921` | `fix(ai-lineup)`: `first_choice_eleven` solves the optimal assignment the squad model already solved (the solver is extracted to `squad_intelligence/assignment.rs` and shared). Adds the `lineup_gaps` measurement and `p4_lineup_gap` study. |
| `7826cbfc` | `fix(squad-intelligence)`: a borrower's model holds the players on loan to it (`Member::loaned_in`), for the horizon years the loan covers, never as the club's to sell. |
| `c2fab7fb` | `feat(ai-manager)`: `ManagerIdentity` (risk, adaptability, rotation, youth trust), derived from the manager id, tilting the weekly tactical plan, rotation and the club's youth focus. Adds three studies. |
| `109a9003` | `fix(loans)`: a prospect lent for his minutes is only borrowed by a club that would play him. Adds the loan and prospect-cohort studies. |
| `6f65c596` | `test(club-strategy)`: the seven required club situations, plus an ignored specification of one gap. |

The three documents are committed with this report (see `git log` for the docs commit).

## 3. Strategic behaviour implemented

1. **One side, one solver.** The squad model valued a signing against an optimal assignment; the picker
   walked slots in order and could spend a striker at left back. They now share one assignment. Measured
   gap before: the fielded side sat 0.78-1.08 raw rating points per slot below the optimum and left
   0.7-0.9 model starters out per club; after: at least the model's, 0.15-0.33 left out (rounding-level).
2. **One squad, one pool.** A club that borrows a striker now sees him, for as long as the loan lasts, and
   sees the vacancy when it ends. Before, 42-58% of AI clubs fielded a loaned-in starter in season 1 that
   their own model did not know existed.
3. **Manager identity.** Four dispositions per manager, persistent from club to club. Risk leans the
   club's blueprint bold or cautious (middle half of managers do not lean); adaptability moves the form
   triggers by up to half a goal (conceding) and a quarter (scoring); rotation moves the rest threshold
   by up to 8 condition points; youth trust shades youth focus by up to 0.15. All are bounded by the
   engine's ration of under-priced dials and by what the squad's legs and pace can run; the neutral
   manager and a club with no manager are exactly as before (asserted by tests).
4. **Purposeful loans.** The lender's and the borrower's idea of a loan for a young player's minutes are
   one function. A borrower takes such a player only if he would start.
5. **Club strategy:** unchanged in mechanism (derived, evolving, no stored state) and now tilted by the
   manager on the youth side. Measured as stable: ambition changes about once per club in three seasons,
   and flips back in under 1% of triples.

## 4. Before and after

Full tables and caveats are in the validation document. Headline figures:

| Measure | Baseline `e5282b67` | P4 | Evidence |
| --- | --- | --- | --- |
| Model starters left out of the fielded side, per club | 0.70-0.92 | 0.15-0.33 | 3 seeds, day 100 |
| Clubs fielding a starter their model could not see (season 1) | 42-58% | 0% (visible) | 3 seeds |
| Idle loans (2+ months, under 180 minutes) | 36.6% | 21.4% | 6 seeds × 3 seasons |
| Age 21 or under: idle loans / minutes per loan day | 53.6% / 4.4 | 26.6% / 8.0 | same |
| All-prospect development (growth, minutes) | +4.36, 677 | +4.34, 676 | same; **unchanged** |
| Distinct tactical plans in the world | 16-24 | 30-35 | 3 seeds |
| Points per game by disposition band, beyond strength | within ±0.017 (placebo) | within ±0.011 | 10 seeds × 2 seasons |
| XI fit / unused joiners (paired difference) | 68.227 / 11.16% | -0.025 (±0.030) / -1.10 pp (±0.90) | 20 seeds × 2 seasons |
| Goals per match | 2.861 | 2.900 (2.907 from the lineup change alone) | 10 seeds × 2 seasons |
| Clubs in debt | 0 | 0 | 20 seeds, to end of season 1 |
| Clubs over 110% of wage budget | 0 | 1 of 1,580 | same |

**Not claimed:** an improvement in squad quality, positional fit, spending or competitive balance. Those
are inside the noise at the sample sizes run. The unused-joiner reduction is small and just clears its
interval.

## 5. Phase by phase: done, partly done, not done

| Phase | Status |
| --- | --- |
| A. Audit | Done. The mismatch is larger than the takeover audit measured, and has a second part (loaned-in players). |
| B. Evolving club strategy | **Partly.** Existing derived strategy audited and measured stable; manager tilt added. Not done: the profile does not reach sales, renewals or lineups; promoted/relegated clubs read their last finish against the wrong division (specified by an ignored test, fix needs a tier on `TeamSeasonRecord`). |
| C. Manager identity | **Partly.** Pre-match planning, rotation, youth. Not done: preferred formations (AI clubs never change formation), pressing preference as its own axis beyond risk, in-match adaptation, appointments by fit, identity that is formed by a career. |
| D. Recruitment against utilisation | **Core done.** Model and picker agree; loans see utilisation. Not done: a per-signing pathway label (starter, rotation, cover, succession, prospect) held against actual use after arrival, and a valid succession-success measure. |
| E. Youth and loans | **Partly.** Loans are purposeful. Not done: any change that makes prospects play more (75% get under 450 minutes a season, unchanged), the sell/renew/release choice for a prospect, reserve-team development. |
| F. Financial coherence | **Not changed.** Existing guards reused; nothing bypassed; no new rule. The profile's means do not reach the sell side. |
| G. Validation | **Partly.** 20 seeds × 2 seasons on the gate world and three-season studies; one shipped season; no 30-season run. |

## 6. Runtime

Quiet machine, release profile as committed, `/usr/bin/time -l`:

| | Instructions | Wall | Peak RSS |
| --- | --- | --- | --- |
| Gate, 20 seeds × 2 seasons, baseline | 2,837.4B | 142.1 s | 112.7 MB |
| Gate, 20 seeds × 2 seasons, P4 | 2,894.4B (+2.0%) | 145.6 s (+2.5%) | 118.2 MB (+4.9%) |
| Shipped (440 clubs), 1 seed × 1 season, baseline | 613.7B | 33.0 s | 154.5 MB |
| Shipped, same, P4 | 620.5B (+1.1%) | 34.0 s | 161.2 MB (+4.3%) |

The F5.5 saving (-26.9%) is preserved; P4 spends about 2% of the F5.5 total. The lineup picker's solve is
about 1.2% of the run on its own. `MARGINAL_CACHE` and `best_assignment` were not touched.

## 7. Tests

| Command | Result |
| --- | --- |
| `cargo test --locked --workspace --no-fail-fast` | **128 targets, 4,756 passed, 0 failed, 83 ignored** (baseline 4,725 / 0 / 76 over 122 targets) |
| `cargo test --locked --lib --features mcp` | 653 passed, 0 failed, 8 ignored |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, and with `--features mcp` | clean |
| `cargo fmt --all` | applied before each commit |
| Frontend (`npm test`, build, lint, knip) | **Not run.** No frontend, IPC, schema or i18n file changed. |
| `cargo deny`, `cargo machete`, `/preflight` as one invocation | **Not run.** |
| `ofm-architecture-reviewer`, `ofm-test-reviewer`, `ofm-dedup-reviewer` | **Not run.** The repository's code-quality rules name them as merge requirements. They should be run on `e5282b67..HEAD` before a pull request. |

The ignored count rose by seven: six new `#[ignore]`d measurement studies in `season-harness/tests/p4_*.rs`
and one ignored specification (`a_promoted_champion_is_still_a_fighter_in_its_new_division`). New passing
tests: first-choice eleven (5), loaned-in players (4), manager identity (3), tactical identity (5),
rotation (3), youth trust (1), loan scenarios (3), club situations (6 plus one ignored), ageing squad (1).

**Discriminating checks done.** The picker trap test failed on the old walk for the predicted reason
(the striker at left back). Putting the old `active_loan` rejection back fails four loaned-in tests;
removing the sale filter fails the "cannot sell a borrowed player" test; zeroing the rotation and
adaptability tilts fails their tests; the prospect-on-the-bench loan test failed before the loan rule.
The identity independence test found a real defect in the existing hash (FNV with the seed folded into
the start state makes two seeds over one id correlate at r = 0.96), fixed with a finaliser.

## 8. Save and reload

No serialized field, migration or SQL change. Identity and club profile are pure functions of persisted
state; the loaned-in change reads `active_loan`, which already persists. Verified: the five-seed gate
report is byte-identical across two runs; `a_career_survives_a_sqlite_reload_at_every_stage_of_four_seasons`
passes. **Not verified:** replaying a save written before P4 for a season and comparing it with the same
save played on the baseline (they will differ, by design, since lineups changed).

## 9. Remaining architectural weaknesses

1. **The study's model-versus-picker columns are invalid on this tree.** `greedy_picks_with` mirrors an
   algorithm the picker no longer runs (and never matched it). `lineup_gap` still uses it for the
   "mirror" column. Both, and the `f54_squad_study` succession lines that read it, should be retired or
   rebuilt on the real picker.
2. **Rounding in the model.** The model's anchored, integer-rounded fits still break ties differently
   from the picker's continuous ones: 0.15-0.33 model starters per club differ. Harmless in raw rating,
   but the two are not the same function.
3. **Opening ratings are stale.** 38% of rostered players have a stored `ovr` more than rounding away from
   their attributes at the opening, consistent by day 100. The model reads stored `ovr`; the engine reads
   attributes. A world-generation issue, not changed.
4. **Promoted and relegated clubs** are misclassified (§5, ignored test).
5. **Identity is assigned, not formed**, and appointments ignore it.
6. **AI formations never change.**
7. **Prospects rarely play** and nothing in the picker can make them.
8. **Size debt, pre-existing:** `ai_select_starting_xi_by` is about 170 lines and `evaluate_transfer_market`
   about 900; both exceed the 100-line ceiling and were extended by a few lines, not decomposed.
9. **Harness hygiene:** six new studies duplicate small helpers (`env_list`, seed parsing). They are
   measurement code under `tests/`; consolidation belongs with epic #589.
10. **One club over 110% of its wage budget** in P4's 20-seed run and none in the baseline's. Unexplained.

## 10. Can the next phase begin?

**Conditionally yes, for backend work.** The central inconsistency is fixed, the determinism and
runtime gates hold, and the full suite is green. Before building further on P4:

1. Run the three reviewer agents on `e5282b67..HEAD` and act on them.
2. Run a 10-season gate run and one multi-season shipped run, and look for drift.
3. Retire the invalid greedy-mirror measurements (§9.1).

Natural next work, in order of what the evidence supports: a development pathway that gives prospects
minutes; the tier on `TeamSeasonRecord` and the promoted-club fix; appointments that match a club's style to
a manager's identity; sales and renewals reading the club profile; AI formation choice, which needs a
measurement of its own first.

The carried-over condition from P2.5 and P3 is unchanged and not addressed here: no person has played the
window, and work that builds on P3 screens should wait for the manual GUI checklist.
