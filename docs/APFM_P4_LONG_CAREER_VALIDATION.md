# APFM P4 long-career validation

What was measured, against which baseline, over how many seeds and seasons, and what it does and does
not support. Companion to `APFM_P4_IMPLEMENTATION_REPORT.md` (what changed) and
`APFM_P4_STRATEGIC_ARCHITECTURE.md` (why).

## Read this first: the scope is short

The brief asked for multi-season validation of a long career. What was run:

| Study | World | Seeds | Seasons | Why that size |
| --- | --- | --- | --- | --- |
| Market, squads, finance (`f54_squad_study`) | gate, 80 clubs | **20** | **2** | 142 s a tree; paired by seed |
| Shipped-world check (same study) | shipped, 440 clubs | 1 | 1 | runtime and no-regression only |
| Loans (`p4_loans`), prospect cohort (`p4_youth`) | gate | 6 | 3 | follows every loan and prospect |
| Results by manager disposition (`p4_results`) | gate | 10 | 2 | 1,936 club-seasons |
| Strategy stability (`p4_strategy_stability`) | gate | 3 | 3 | 20 samples a club |
| Manager tactical diversity (`p4_manager_diversity`) | gate | 3 | 1 | plans at days 30, 100, 200 |
| Model against picker (`p4_lineup_gap`) | gate | 3 | 2 | every club read three times a seed |

**No 30-season run and no full shipped-world multi-season run were done.** The F5.4.1 shipped 30-season
run took 3 h 22 m; this session did not repeat it. Nothing below speaks to what happens in season 10 or
season 30: a drift that needs a decade (squad ageing across a whole world, reputation spread, the
loan market's long-run shape) is not covered. The next phase should run it before building on P4.

Baseline for every comparison: **`e5282b67`**, the integrated F5.5 tree, built from a detached worktree
and run with the same binary names and seeds. Its headline (699.2B instructions, 34.5 s for five gate seeds
over two seasons) reproduced the figures in `APFM_P4_INTEGRATION_BASELINE.md` exactly before any change was
made. Seeds are paired: the same seed in the baseline and in P4 starts the same world, but every change that
touches a lineup reshuffles the match random stream, so a single seed is a different history, not a
controlled replay. That is why differences below carry confidence intervals over seeds.

## 1. Starting-XI quality and squad fit

Model side against the side the picker fields, same healthy seniors, game's raw slot rating
(`p4_lineup_gap`, 3 seeds). Day 100 of seasons 1 and 2 (the opening is left out: stored ratings are stale for
38% of players there, see the architecture document §2).

| | Before (picker = slot walk) | After (picker = shared optimal assignment) |
| --- | --- | --- |
| Clubs whose fielded side is not the model's side | 87-96% | 34-45% (rounding-level) |
| Model starters left out of the fielded side, per club | 0.70-0.92 | 0.15-0.33 |
| Raw rating per slot the fielded side leaves behind the optimum, mean | 0.78-1.08 | -0.01 to -0.03 (the picker is at least the model's; read after the picker change) |
| Same, p90 | 1.67-2.40 | 0.0 |
| Gain from the change, by club strength (weakest to strongest quarter, raw per slot; mean of the nine seed-by-time rows) | | +1.08, +0.91, +1.01, +0.83 (spread evenly; slightly larger for weaker clubs) |

The market's own squad-quality proxy over 20 gate seeds × 2 seasons (paired difference, P4 minus
baseline, with a 95% interval over seeds):

| Metric | Baseline | P4 | Difference |
| --- | --- | --- | --- |
| XI fit (mean over club-seasons) | 68.227 | 68.202 | -0.025 (±0.030) |
| Clubs with a weak slot | 15.5% | 16.5% | +1.0 pp (±0.9) |
| Out-of-group starters per club | 1.534 | 1.551 | +0.018 (±0.022) |
| Usable bench | 9.829 | 9.832 | +0.002 (±0.049) |

No measurable change in squad quality or positional fit. The weak-slot rise is inside its interval and
is flagged rather than explained.

## 2. Unused signings

The harness counts a joiner as unused by what he did after arriving and groups deals by the buyer's
intended role and the window they came in; this was used as it stands. It does not call every
substitute or prospect a failure: a depth signing who covers is used, a prospect is judged by role.
20 seeds × 2 seasons:

| | Baseline | P4 | Difference |
| --- | --- | --- | --- |
| Unused joiners, all arrivals | 11.16% | 10.07% | -1.10 pp (±0.90) |
| Paid transfers, unused (club-weighted) | 33.5% | 29.7% | |
| Free agents, unused | 9.7% | 9.0% | |
| Paid transfers per club-season | 0.090 | 0.079 | -0.011 (±0.010) |
| Fees per club-season (£m) | 0.111 | 0.096 | -0.015 (±0.016) |

The unused-joiner reduction is small, and just clears its interval (-2.0 to -0.2). It is what a more
consistent model and picker should produce (a signing is valued by the start he would actually get); it is
not a large effect and should not be sold as one. No quota or cap was added: the 12% fewer paid
transfers is an outcome, not a rule.

**Succession success could not be measured on this tree.** The study's "model sees a start / the real
picker would give him one / model-only" columns read the model's greedy mirror, which is not the picker
(before or after). They are not reported here. A valid replacement needs the model's start compared with
the real picker over a signing's first season, which `lineup_gap` could be extended to do.

## 3. Youth minutes and loan outcomes

Every loan of an AI-managed player followed day by day (`p4_loans`, 6 seeds × 3 seasons); an **idle**
loan lasts two months or more and gives the player under 180 minutes.

| | Baseline | P4 |
| --- | --- | --- |
| Loans ended | 1,305 | 974 |
| Idle loans, all | 36.6% | 21.4% |
| Age 21 or under: loans / idle / minutes per loan day / rating change | 673 / 53.6% / 4.4 / +1.48 | 353 / 26.6% / 8.0 / +1.91 |
| Room to grow (6+): loans / idle / minutes per loan day / rating change | 596 / 57.4% / 3.8 / +1.54 | 243 / 28.4% / 8.2 / +2.30 |
| Age 25 or over: loans / idle | 423 / 14.9% | 400 / 13.5% |

Loans are far more purposeful, and there are fewer of them (prospect loans fall by half or more). The
question that decides whether this is a gain is whether the prospects who are no longer lent develop less.
`p4_youth` follows every prospect (21 or under, 6+ to grow), loaned or not, a season at a time (6 seeds × 3
seasons, ~12,600 player-seasons each):

| | Baseline | P4 |
| --- | --- | --- |
| Rating growth per season | +4.36 | +4.34 |
| Minutes per season | 677 | 676 |
| Under 450 minutes in a season | 75.3% | 75.7% |

**Overall youth development did not change.** The change removes loans that were not doing what they were
for and costs no measurable development. It is an efficiency and coherence gain, not a development gain,
and the report does not claim otherwise. That three quarters of prospects play under 450 minutes a season
in both trees is the larger finding and is not addressed by P4 (see §8).

## 4. Transfer expenditure and wage sustainability

The study's finance table is printed for the opening and the end of season 1 only (its season list is
0, 1, 5, 10, 20, 30), so this is **one season of evidence, not two**. Over 20 gate seeds (1,580
club-seasons a tree): no club in debt in either tree at either point; median wage bill against wage
budget 0.93-0.95 at the opening and 0.99-1.01 at the end of season 1 in both. Clubs over 110% of their
wage budget at the end of season 1: **none in the baseline, one in P4** (1 of 1,580). One club is not a
trend and is not explained; it is reported because the baseline had none. Fees per club-season are in §2.
One shipped-world season: fees £0.10m → £0.09m per club-season, unused joiners 12.2% → 12.0%.

## 5. Manager tactical diversity, and whether any temperament wins

Distinct tactical plans running in the world at days 30, 100 and 200 of season 1 (`p4_manager_diversity`,
3 seeds):

| | Baseline | P4 |
| --- | --- | --- |
| Distinct plans in the world | 16-24 | 30-35 |
| Clubs of one style, distinct plans: Defensive | 2 of 12-21 | 5-6 of 12-21 |
| Clubs of one style, distinct plans: Balanced | 2-4 of 10-16 | 3-6 of 10-16 |
| Plans differing from the style's blueprint | 66-80% | 68-86% |

Managers decide differently. The condition that matters is whether one temperament simply wins:
`p4_results` (10 seeds × 2 seasons, 1,936 club-seasons) compares each club's points per game with what its
strength predicts, by the disposition of its manager. Mean residual, points per game, with standard error:

| Band | Baseline (placebo: nothing reads dispositions) | P4 |
| --- | --- | --- |
| Risk, bold / cautious | +0.001 / -0.003 (±0.010-0.012) | -0.000 / +0.011 (±0.010-0.012) |
| Adaptability, high / low | -0.017 / +0.005 | -0.008 / +0.003 |
| Rotation, high / low | +0.000 / -0.006 | +0.009 / +0.007 |

Every band is within about ±0.02 points per game of zero, the same noise floor as the placebo. **No
disposition wins beyond strength**, so P4 did not make one tactic the strongest and every manager converge
on it. It also means the dispositions are not measurably valuable or harmful; their effect is in what clubs
do, not how they finish.

League scoring (same runs): goals per match 2.861 (baseline), **2.907 with only the lineup change**,
2.900 with everything. The +1.4% is the lineup change (better-placed players score more); the identity
slices are neutral on it. Home wins 43.6% / 43.5% / 43.2%, draws 24.7% / 24.8% / 25.0%.

## 6. Club strategy changes

`p4_strategy_stability` (3 seeds × 3 seasons × 20 samples): a club's ambition changes in about 5% of
50-day intervals and its means in about 6.5%, roughly once per club in three seasons, and flips back in
under 1% of triples. Details in the architecture document §4. The seven required situations (wealthy
contender, financially constrained, promoted, youth-built, ageing, poor results, new manager) are unit
tests in `transfers/strategy.rs` and `squad_intelligence/scenarios.rs`; all pass except the promoted
champion, which is an executable specification of a real gap and `#[ignore]`d with its reason.

## 7. Competitive balance, determinism, save and reload, runtime

- **Competitive balance.** Titles by founding-reputation quartile (weakest to strongest), 20 seeds × 2
  seasons, 200 titles: baseline 10.0 / 20.0 / 32.0 / 38.0%; P4 12.0 / 22.5 / 30.0 / 35.5%. Slightly flatter,
  inside the noise of 200 titles. Correlation of XI quality with points per game: 0.364 → 0.343 (40 seasons
  each, standard error about 0.03). No claim.
- **Determinism.** The final tree's five-seed, two-season report is byte-identical across two runs (514
  lines, SHA-256 prefix `f4645439d21f`), as the slice-one tree's was.
- **Save and reload.** P4 added no serialized field and no migration: the manager identity and the club
  profile are pure functions of persisted state, so a reload cannot disagree about them. The loaned-in
  change reads `active_loan`, which already persists. `season_lifecycle`'s four-season SQLite reload at
  every stage passes on the final tree (see the report for the run). What this does *not* show is that a
  mid-season reload of a save made before P4 plays on identically: identities are derived from ids so the
  old save gets them, and no test replays an old save for a season.
- **Runtime.** Same machine, quiet, `/usr/bin/time -l`, release profile as committed.

| | Instructions retired | Wall | Peak RSS |
| --- | --- | --- | --- |
| Gate, 5 seeds × 2 seasons: baseline | 699.2B | 34.5 s | 104.3 MB |
| Gate, 20 seeds × 2 seasons: baseline | 2,837.4B | 142.1 s | 112.7 MB |
| Gate, 20 seeds × 2 seasons: P4 | 2,894.4B (+2.0%) | 145.6 s (+2.5%) | 118.2 MB (+4.9%) |
| Shipped, 1 seed × 1 season: baseline | 613.7B | 33.0 s | 154.5 MB |
| Shipped, 1 seed × 1 season: P4 | 620.5B (+1.1%) | 34.0 s (+3%) | 161.2 MB (+4.3%) |

The F5.5 gain (-26.9% instructions) is preserved: P4 costs about 2% of the F5.5 total. The cost is the
shared solver in the lineup picker and the borrower's larger model. Peak memory grows by 4-5%.

## 8. What this does not show

- Nothing past season 3, and nothing on the shipped world beyond one season.
- Whether prospects should play more. Three quarters get under 450 minutes a season, unchanged. The picker
  chooses on merit and has no youth preference, and the youth-trust disposition only reaches the market
  side. A development pathway that actually plays prospects (reserve fixtures, youth cup minutes, a manager
  who gives a first-team chance) is the largest sporting gap this phase found and did not close.
- Financial coherence beyond not breaking it. The market's existing guards (budget, wage ceiling,
  overstretched checks) were reused and no club ended in debt, but P4 did not add a new financial rule, and
  the sales and renewal side still does not read the club profile.
- The in-match manager. Only pre-match planning, rotation and the weekly review were touched.
- Anything about the 2D viewer or M12, which were not touched. The in-house engine decides AI fixtures.
