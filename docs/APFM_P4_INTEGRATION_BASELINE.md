# APFM P4 integration baseline

Consolidation checkpoint: DeepSeek's F5.5 performance branch merged into the Claude playable-career
branch (P3). No new development in this step.

## 1. The merge

| | Commit |
| --- | --- |
| **Merge commit** | `ce6c698c` `merge: integrate F5.5 squad intelligence and market performance` |
| Parent 1 (Claude, P3) | `840d9975` `docs(playable-career): P3 management UX report` |
| Parent 2 (DeepSeek, F5.5) | `bb9b72b2` `docs(perf): record the F5.5 market performance work` |
| Common ancestor | `92b2c616` `merge: integrate F5.4.2-F5.4.5 sporting AI` |

- Branch: `claude/apfm-playable-career`. Not merged into `apfm/main`. Not pushed.
- A true merge commit (`--no-ff`); the five F5.5 commits are preserved:
  `14197500`, `f18566d1`, `8187142a`, `d4ece824`, `fff29ee0`, then the doc `bb9b72b2`.
- No file is touched by both sides: F5.5 changed 6 files (5 Rust in `ofm_core`, 1 doc); P3 changed
  57 (frontend, i18n, `src-tauri/src`, docs). `git merge-tree` produced a clean tree before the merge.
- M12 and the external viewer were not touched. This worktree has no
  `src-tauri/.cargo/config.toml`, so both builds below use the pinned engine.

## 2. F5.5 claims checked against the code

| Claim | Verdict | What was checked |
| --- | --- | --- |
| Candidate evaluation without cloning `Player` | **Holds** | Old: clone, clear `active_loan`, `Member::of`, then recompute `shift` with a new contract. New: same guards (`Senior`, not `retired`; the `active_loan` guard is the one dropped on purpose), same fields. `PlannedPlayer::of`, `fits`, `absence_of`, `natural_ovr` and `contract_end()` do not read `active_loan`. Old `out_now` was `shift[0].is_none()`; `projected_shifts` always sets `shift[0] = Some(0.0)`, so it was true only for a long injury, which is exactly what the new code tests. |
| Reuse of the squad's own year view | **Holds** | `reuses_base_cover` requires an `Unchanged` chain, every slot already covered, and the newcomer no better than the cover anywhere. The newcomer is appended last, so on equal level it loses the bench-position tie-break; every one of his pairings then meets an already-covered slot, so the accepted pairing sequence is the base's. A bit-for-bit oracle test (`evaluate_with_full`, 80 squads) and a counter test cover it. |
| Faster cover-pair sort | **Holds** | `pairing_key` maps `f32` bits to the `total_cmp` integer order, inverts it for level-descending, then slot, then bench position: the old comparator's order. Fields fit (rank 32 bits at shift 20, slot 4 bits, position 16 bits). Cover level is read back from the member. |
| `StableHasher` fingerprint | **Holds** | Same bytes in the same order; `stable_hash` is now a wrapper over it. The pinned "save file can rely on this number" test passes untouched. `ClubSquad::sales` reads the stored fingerprint, which is set once from `model.fingerprint()` at construction. |
| No repeated full-list loan scans | **Holds** | `LoanTarget.player_index` is the place in `Game::players`; the only structural mutations of that list under `transfers/` are appends (one `truncate`, in a test), so indices stay valid, and `try_ai_loan` still checks the id. |
| −31.5% instructions, same binary | **Not independently reproduced** | The five switches were removed from the final code, so the same-binary experiment cannot be re-run. Across the two real builds the saving is 26.9% (section 4). |
| Sporting output byte-identical | **Confirmed** | Section 4. |

## 3. Validation of the merged tree (`ce6c698c`)

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets` | clean; also clean with `-- -D warnings` |
| `cargo clippy --workspace --all-targets --features mcp -- -D warnings` | clean |
| `cargo test --workspace` | **4,725 passed, 0 failed, 76 ignored**, 122 targets (`ofm_core` lib 2,063; Tauri lib 567; `db` 441) |
| `cargo test --lib --features mcp` | 653 passed, 0 failed, 8 ignored |
| `transfers_tests` | 143 passed |
| F5.5 regression tests (8 named in the F5.5 report) | all 8 ran and passed |
| `delegated_career`, `delegated_recruitment`, `measure_delegation_modes` | 1, 2, 2 passed |
| `season_lifecycle` (multi-season) | 6 passed |
| `squad_floor_seasons`, `end_of_season_tests` | 9, 115 passed |
| `npm test` | 340 files passed, 1 skipped; **2,993 tests passed**, 4 skipped |
| `npm run build` (`tsc && vite build`) | passes |
| `npm run format:check`, `lint`, `knip` | pass (Biome 2.5.5, the real one) |

The 76 ignored Rust tests are `#[ignore]`d measurement studies (for example `f54_squad_study`);
the performance study below ran that one explicitly.

## 4. Performance and sporting-equivalence evidence

Setup: `f54_squad_study`, `F54_WORLD=gate F54_SEEDS=1,2,3,4,5 F54_SEASONS=2`, release profile as
committed, rustc 1.95.0, macOS arm64, run from `src-tauri/`. Baseline built from a detached
worktree at `92b2c616`; merged binary built from this tree. Order A/B/B/A, `/usr/bin/time -l`.

| Binary | Instructions retired | Real | Per-seed simulation | Peak RSS |
| --- | ---: | ---: | --- | ---: |
| Baseline `92b2c616` (run 1 / run 2) | 956.5B / 956.1B | 50.0 s / 49.7 s | 9.3 to 10.1 s | 107.5 MB |
| Merged `ce6c698c` (run 1 / run 2) | 699.2B / 699.2B | 34.9 s / 34.8 s | 6.7 to 7.0 s | 104.1 MB |
| Change | **−26.9%** | **−30%** | | −3% |

- These are two separate builds, so the caveat in the F5.5 report applies: thin LTO with one
  codegen unit makes instruction counts between binaries indicative, not exact. Both builds agree
  with the F5.5 report's own separate-build figures (957.6B and 699.4B), and run-to-run spread within a
  binary is 0.05%. The F5.5 headline of −31.5% is a same-binary figure; the build-to-build saving a
  player's build will see is about 27%.
- **Sporting equivalence:** the study report (516 lines per run, with the `… in X.XXXs` timing
  suffixes removed) is **byte-identical across all four runs**, baseline and merged, same SHA-256
  `1e429c6f…`. That includes the seed-1 market participation line
  (`teams 9638 | positional need 377 | squad need 7450 | loan need 9170 | shopped 4162 | review day
  1368 | closing need 1243 | whole market 474`), which matches the F5.5 report. The two baseline runs
  agreeing with each other shows the report is deterministic, so identity is meaningful.
- Scope: gate world, five seeds, two seasons. Not the shipped 440-club world, not 30 seasons.

## 5. Remaining risks

1. **No person has played the window.** Still true from P2.5 and P3: the manual GUI acceptance
   checklist (sections 1, 4, 5, 8 at minimum) has not been done, and P3 added screens (column chooser,
   focus hand-off, failure notices) that nobody has seen working. Automation cannot judge contrast,
   a screen reader on the alert region, or the chooser in a narrow window.
2. **Equivalence is evidenced over five gate-world seeds and two seasons**, plus the oracle tests.
   The shipped world and a long career were not re-run on the merged tree.
3. **Two assumptions are encoded in F5.5**, both tested today and both worth remembering:
   the `fits` de-duplication assumes a slot's rating depends only on its position (true of
   `positional_fit_for_assignment` now); the loan target index assumes `Game::players` is only
   appended to during a sweep (the id check turns a violation into a skipped loan, not a wrong one,
   but a skipped loan would change behaviour silently).
4. **Performance remains above F5.3** (about 1.8× per the F5.5 report). `best_assignment` and
   `year_view_from` are the largest remaining frames and were deliberately not changed.
   `MARGINAL_CACHE` is still one process-global mutex.
5. **Not run in this step:** the repository's reviewer agents (`ofm-architecture-reviewer`,
   `ofm-dedup-reviewer`, `ofm-test-reviewer`) and the `/preflight` evidence list; `cargo-machete`;
   the i18n audit beyond the vitest gates inside `npm test`. F5.5 itself adds no player-visible
   strings, no schema change and no IPC change, so `i18n-auditor` and `ui-accessibility-reviewer`
   have nothing new to read.
6. `evaluate_transfer_market` is about 900 lines (per the F5.5 report, not re-measured here), well over
   the repository's 100-line function ceiling. F5.5 extracted one helper from it
   (`collect_loan_targets`) but did not decompose it, and this step did not either.

## 6. Exact starting point for P4

- Repository: `~/apfm-migration/apfm-claude-playable`, branch `claude/apfm-playable-career`.
- Start from **`ce6c698c`** (this merge) plus the commit that adds this report.
- Working tree clean; nothing pushed; `apfm/main` unchanged.
- `docs/APFM_PLAYABLE_CAREER_IMPLEMENTATION_PLAN.md` does not name a "P4". What it leaves open:
  Stage 4 (first human playthrough, per the checklist), the transfer-table half of Stage 5,
  Stage 6 (model and picker consistency, needs the owner's decision first), Stage 7 (engine versus
  M12 study) and Stage 8 (shipped-world and long-career performance).
- Stage 8's "recorded baseline" should now be this document's section 4 for the gate world
  (699.2B instructions, 34.8 s, report hash above), not the F5.4 figures.
- Per the P3 report (section 8): P4 work that does not depend on how screens behave in a window can
  proceed on the same terms as P3. P4 work that builds on the P3 screens should wait for one person
  to complete the checklist sections above; that decision is the owner's.

## 7. Decision

**GO for P4**, with the condition in section 6: backend and study work may start now; work that builds
on the P3 UI should follow a human pass over the checklist.
