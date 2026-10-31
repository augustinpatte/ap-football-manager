# APFM Run 3 — integration of Run 1 (transfers) and Run 2 (tactics)

Branch `claude/run3-integration`, derived from `integration/run1-run2`, merging
`codex/run2-tactics`. Target of the PR: `integration/run1-run2`. `apfm/main` untouched.

## Branch histories

- Common ancestor of both: `4e5dd35` (`apfm/main`, schema v061).
- Run 1 (`integration/run1-run2`): 8 commits — signing bonuses, fee instalments, sell-on clauses,
  player consent, stepping-stone promise, deadline day, inbox actions.
- Run 2 (`codex/run2-tactics`): 2 commits — dual formations (in/out of possession), tactical
  presets, custom tactics, M12 integration.
- Files changed by both: 16 (migrations, `save_manager.rs`, `lib.rs`, `store/types.ts`, 12 locale
  files). Git reported **one** textual conflict, `db/src/migrations.rs`; everything else merged
  cleanly and was checked semantically (see below).

## Migration resolution

Both branches had created `v062_*.sql` (neither was released: `apfm/main` is at v061).

| Version | File | Origin |
|---|---|---|
| v062 | `v062_transfer_obligations.sql` | Run 1, unchanged |
| v063 | `v063_out_of_possession_formation.sql` | Run 2, **renamed** from `v062_…`; SQL byte-identical, same fingerprint |

Why this order: the integration branch is Run 1's, so Run 1 development saves need no handling.

Changes: `EXPECTED_MIGRATION_COUNT` 61 → 63, `SHIPPED` fingerprint table has both rows,
registration list updated. No released migration was modified.

### Saves written by development builds

A Run 2 dev save claims `user_version = 62`, holds `teams.out_of_possession_formation` and has no
`transfer_obligations` table. Under the new numbering that is neither an honest v62 nor v63: it
would fail on a duplicate column (`GameDatabase::open`) or be refused as an incompatible layout
(`open_save`). Fix (`game_database.rs`, `migrations.rs`): a save with exactly that signature gets
a copy `<save>.db.pre-v063.bak`, then the missing, purely additive v062 statement runs in one
transaction and `user_version` becomes 63. Nothing is dropped or rewritten. If the backup cannot
be written the save is left untouched. Any other v62 layout (e.g. both column and table) is still
refused unmodified. Remove the repair once no Run 2 dev save is expected to exist.

## Semantic audit of the overlaps

| Area | Finding |
|---|---|
| Domain types | `Team` (Run 2: `out_of_possession_formation`) and `Player`/`Game`/`finance` (Run 1) are disjoint; both fields carry `#[serde(default)]`. |
| Persistence | `team_repo.rs` (Run 2) and `transfer_obligation_repo.rs`/`player_repo.rs` (Run 1) touch different tables/columns. Verified by round-trip tests below. |
| Tauri commands | `lib.rs` auto-merge registers both sets (`set_out_of_possession_formation`, `apply_custom_tactic` and Run 1's contract/transfer/message commands). It compiles. |
| Frontend | `store/types.ts` holds both field sets; `tsc` and the build are clean. |
| Translations | 12 locale files auto-merged; no duplicate keys, all valid JSON; `src/i18n` coverage tests pass. |
| Transfer → squad → tactics | A sold player is removed from the starting XI (existing code); the new scenario test checks the user's two-shape tactic is untouched by that. |

No Run 1 or Run 2 feature was removed.

## Tests added

`src-tauri/crates/db/src/game_database.rs`:
- `a_run1_development_save_upgrades_to_the_integrated_schema_without_loss`
- `a_run2_development_save_is_repaired_to_the_integrated_schema_without_loss` (also checks the backup and a second open)
- `a_run2_development_save_also_opens_through_the_plain_entry_point`
- `a_v62_save_that_matches_neither_dev_build_is_refused_untouched`
- `a_v61_save_upgrades_through_both_new_migrations`

`src-tauri/crates/db/src/migrations.rs`:
- `transfer_obligations_and_out_of_possession_formation_have_distinct_consecutive_versions`

`src-tauri/crates/db/tests/run3_integration_tests.rs` (real `.db`, fresh `SaveManager` readers):
- `a_career_with_open_transfer_obligations_and_custom_tactics_survives_two_round_trips`
- `an_instalment_falling_due_after_a_reload_is_paid_once_and_tactics_are_kept` (calendar progression through `process_day`)
- `selling_a_fielded_signing_settles_the_clause_and_leaves_the_tactics_alone` (inbox-style offer answered, XI, sell-on, save/reload)

Red-before-green: the two Run 2 repair tests were run with the repair call disabled in a scratch
edit and **failed**; the edit was reverted. The other new tests were not shown failing against a
removed fix (they guard the merge as a whole, there is no single fix to remove).

## Verification

Environment: Linux container, Rust 1.95.0 (pinned), Node 22.23. GTK/WebKit dev packages were
installed with `apt` to build the Tauri crate.

| Command | Result |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml --workspace --no-fail-fast` | 4068 passed, **6 failed**, 53 ignored (all 6 failures are the M12 stub, below) |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib --features mcp` | 619 passed, **5 failed** (same stub cause), 1 ignored |
| `cargo test -p db` (of the above) | 389 unit + 3 integration + others, 0 failed |
| `cargo clippy … --locked --workspace --all-targets -- -D warnings` (default and `--features mcp`) | clean |
| `cargo fmt … --check` | clean for repo files |
| `npx tsc --noEmit` | clean |
| `npm run build` | succeeds |
| `npm run format:check` (Biome) | 793 files, no fixes |
| `npm test` | 311 files / 2745 tests: 2744 passed, **1 failed** (below) |

### Failures, all pre-existing or environmental

1. **M12 engine absent.** `ofm_core` depends on `apfm-match-lab` by path (`../../../../../apfm-match-lab`,
   outside the repo, `UNLICENSED`). It is missing here, as it was for Run 1. I built against a
   throwaway stub at `/home/apfm-match-lab`, **outside the repository and not committed**, whose
   simulation always returns an error. Failing: `spatial_runner::tests::m12_execution_projects_a_deterministic_coherent_match_report`
   and five `commands::spatial_match::tests::*` finalisation tests. Their messages read
   `apfm-match-lab stub: real M12 engine unavailable`. They are not evidence either way; they must be
   run with the real crate.
2. **`src/pages/Dashboard.test.tsx` › "clears a stale active save id when a later refresh cannot load it"**
   fails identically on `4e5dd35` (`apfm/main`), on `integration/run1-run2` and on `codex/run2-tactics`
   (checked in disposable worktrees). Not caused by this integration; not fixed here.

## Not done / residual limitations

- The real M12 simulation, and therefore Run 2's defensive-shape effect inside an actual match,
  was not exercised. Run 2's own adapter tests (which don't need the simulation) pass.
- The Tauri window and the UI were not run; frontend coverage is vitest + type check + build only.
- `npm run audit:i18n` (advisory) and the review agents (`ofm-architecture-reviewer`, `i18n-auditor`,
  `ofm-test-reviewer`, …) were not run.
- The end-to-end scenarios drive the core API (`make_transfer_bid_with_terms`, `respond_to_offer`,
  `process_day`) with the real persistence layer, not the Tauri commands.
- No user save database or `/Users/augustinpatte/apfootballmanager` was touched; the repair was
  verified on synthetic saves only. Back up real dev saves before first opening them.

## Changed files

41 files against `integration/run1-run2` (+1559 / −161): Run 2's 38 files (tactics UI, services,
`team_repo`, `custom_tactic*`, `spatial_adapter`, `commands/squad.rs`, locales, `docs/ARCHITECTURE.md`),
plus `Cargo.lock`, and the integration work itself:
`db/src/migrations.rs`, `db/src/game_database.rs`, `db/src/sql/v063_out_of_possession_formation.sql`
(renamed), `db/tests/run3_integration_tests.rs` (new), this report.
