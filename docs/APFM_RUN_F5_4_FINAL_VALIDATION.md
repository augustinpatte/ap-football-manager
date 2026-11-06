# APFM F5.4.1 final validation

## Recovery and scope

Validation resumed on `claude/run-f5-4-squad-intelligence-and-delegation` from F5.4 validation
commit `1a2568d509802b30a371e4eeafa237af5752bd5a`; F5.3 baseline: `ae621d1f`.
The recovery checkpoint was `213e101354d6911cb68ed80af2b7412a4255ccb7`. No user save,
2D viewer, M12 source or `apfm/main` was modified; persistence scenarios use temporary SQLite.

## Confirmed regressions and corrections

All three F5.3-only market regressions pass through `evaluate_transfer_market`.

1. Geographic affinity multiplied sporting value as well as scouting interest. `target_rank` now
   discounts interest only: a clear foreign upgrade can win, but a close domestic option remains preferred.
2. The four-point cross-border exception admitted cheap marginal fillers. `CROSS_BORDER_MIN_GAIN`
   is seven; thin groups and valuable targets keep their existing exceptions.
3. A scoped borrower did not recognise a listed-loan opportunity. The market now cheaply indexes
   listed loans by position group and immediately includes clubs for which one clears the existing
   depth gate. Execution keeps both that gate and `BuyerView::worth_borrowing`, then uses the ordinary
   consent, affordability, registration and return paths. A suitable loan is considered as a
   short-term market solution before an unrelated permanent AI-to-AI purchase can suppress it.

`a_weak_attack_enters_the_market_even_when_legacy_depth_is_complete` remains green: the actual
sweep signs a striker for a club whose goalkeepers mask its attack under legacy depth. The shared
need urgency threshold remains 7.0; no evidence justified changing it.

## Technical validation

| Check | Result |
| --- | --- |
| Rust format, workspace check and workspace Clippy (`-D warnings`) | pass |
| `cargo test --manifest-path src-tauri/Cargo.toml --workspace --quiet` | pass; no failures; environment-gated tests remain ignored |
| Core market realism suite | pass, 15/15 |
| `npm test -- --run` | pass: 332 files, 2,920 tests; 1 file / 4 tests skipped; 96.33 s |
| `npm run build` | pass |

## Integrated transfers and delegation

- `delegation_scenarios`: pass, 19/19: prudent proposals, Total acceptance/rejection/counteroffers,
  listings, protected players, manual changes, reload during negotiations and duplicate prevention.
- `delegated_career`: pass: full delegated season, reload and rollover.
- `delegated_recruitment`: pass: complete season, transfer-window reload and rollover. The scenario
  now explicitly tests the fee-limited autonomous Custom recruitment duty. Total is intentionally
  governed by live club books rather than a per-deal ceiling; the strict £4.0m ceiling assertion
  is retained for the policy that actually promises it.
- `measure_delegation_modes`: pass, 2/2. In 120 days, Full executed 69 actions, Approval 46 and
  retained two proposals, and Manual executed none.

## F5.3 comparison and performance

Identical release gate probe (`seed=1`, two seasons, 80 clubs):

| Measure | F5.3 | F5.4 |
| --- | ---: | ---: |
| XI fit | 67.82 | 67.86 |
| Weak-slot clubs | 20.3% | 21.5% |
| Out-of-group starters/club | 1.519 | 1.513 |
| Usable bench | 9.94 | 9.89 |
| Thin groups/club | 0.000 | 0.006 |
| Unused joiners | 16.8% | 19.6% |
| Fees/club-season | £0.05m | £0.31m |
| Runtime | 3.57 s | 15.50 s |

One seed does not prove match or competitiveness improvement. It shows stable short-run finance
(no debt or wage-budget violations), a 4.3x runtime increase, and small deterioration in weak-slot
and unused-joiner proxies.

The F5.4 shipped-world release study (`seed=1`, 440 clubs, 30 seasons) passed but took
**12,164.34 s (3 h 22 m 44 s)**. It produced 13,170 club-seasons: XI fit 70.05, weak-slot clubs
4.2%, out-of-group starters 0.899/club, keeper holes 0.0%, usable bench 11.44, thin groups 0.003,
unused joiners 24.4%, and £2.15m fees/club-season. It ended with zero debt and zero clubs above the
1.1 wage-budget threshold. The equivalent F5.3 shipped run was not repeated: the F5.4 time is
already operationally unacceptable, so another multi-hour run cannot make the release decision better.

## Decision

**NO-GO.** Functional market and delegation regressions are fixed and technical checks pass, but a
single shipped-world 30-season seed takes more than three hours. Profile and bound the daily market
sweep (especially listed-loan participation) against F5.3, then repeat a comparable multi-seed
shipped-world benchmark before release.

Starting validation commit: `1a2568d509802b30a371e4eeafa237af5752bd5a`.
Final F5.4.1 validation commit: `54be3a6f09864a60ca465abb2cbb669cca93395d`.
