# APFM F5.4 final validation

## Recovery

Validation began on `claude/run-f5-4-squad-intelligence-and-delegation` at
`213e101354d6911cb68ed80af2b7412a4255ccb7` (`chore(f5.4): checkpoint recovered squad intelligence delegation work`).
The worktree was clean. The recovery reflog records that commit at 2026-11-05 19:12:18 -0600;
the F5.3 comparison base is `ae621d1f`.

## Independent findings and correction

`transfers/market.rs` used only `position_depths` / `has_unmet_need` to decide whether an AI club
entered the early-window market. That misses a complete but badly imbalanced squad: a club with two
elite keepers and a weak attack can have no legacy depth shortage. The production sweep already builds
each AI club's planning `ClubSquad` to price sales. It now retains only urgent shared needs (severity
at least 7.0) for participation, avoiding a second model pass and leaving smaller needs to normal
review and target appraisal.

Regression `realism::a_weak_attack_enters_the_market_even_when_legacy_depth_is_complete` was first
observed failing: the striker remained at `seller`. It now proves that the early-window buyer is
evaluated because of its attacking need and that the next ordinary market sweep completes the striker
transfer through discovery, ranking, consent, finance and execution.

The full suite also exposed an invalid debug assertion in Squad Intelligence sale assessment: a sparse
squad's deterministic cover assignment can value a hypothetical cheap replacement below an empty slot.
The assertion now states the valid non-negative invariant without changing the established sale value.

## Tests executed

| Command / scenario | Result |
| --- | --- |
| `cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --test transfers_tests weak_attack_enters_the_market_even_when_legacy_depth_is_complete -- --nocapture` | pass, 1 test |
| `cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --test determinism_tests -- --nocapture` | pass, 8 tests |
| `npm test -- --run` | pass, 332 files / 2,920 tests (1 file / 4 tests skipped) |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | initially failed on this audit's Rust formatting; `cargo fmt --all` was applied and the check must be repeated before a GO |
| `cargo test --manifest-path src-tauri/Cargo.toml --workspace` | **fail**: `determinism_tests` had 7 failures before the assertion correction; the correction's targeted determinism suite passes, but the full workspace has not been re-run to completion afterward |
| F5.4 `cargo test ... --test transfers_tests realism::` | **fail**, 12 passed / 3 failed |
| F5.3 baseline `cargo test ... --test transfers_tests realism::` | pass, 14 passed |

The three remaining F5.4-only market failures are:

1. `a_small_french_club_prefers_a_local_solution_unless_the_foreign_one_is_far_better`
2. `a_small_club_does_not_go_abroad_for_a_cheap_marginal_filler`
3. `a_young_bench_player_goes_on_loan_and_comes_back`

They affect country-aware target selection and AI loans, so sales, complete-window delegation and
multi-window financial claims are not yet validated. Build, workspace Clippy, the repeated format
check, and season-harness F5.3/F5.4 benchmarks are deliberately not reported as passing.

## Delegation and sales audit

The existing F5.4 scenario suite covers prudent proposals and approvals, Total acceptance, rejection,
counteroffers, listings, protected players, manual overrides, and persisted delegated careers. Those
scenarios cannot establish release readiness while the general AI market scenarios above fail. No user
saves were opened: all exercised persistence scenarios use temporary databases.

## Comparison and performance

No F5.3/F5.4 season comparison or 30-season shipped-world benchmark was run after the market
regression was demonstrated. Reporting performance, financial sustainability, turnover, positional
fit, or league competitiveness now would be misleading.

## Decision

**NO-GO.** F5.4 has one confirmed and corrected early-market participation defect, but it still
regresses three F5.3 market behaviours. Resolve those production-path failures, rerun the complete
suite and required lint/build checks, then run the small multi-season and 30-season shipped-world
comparisons before release validation or merge.

## Commits

Starting checkpoint: `213e101354d6911cb68ed80af2b7412a4255ccb7`.
Final audit commit: recorded after this report is committed.
