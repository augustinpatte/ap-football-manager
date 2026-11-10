# APFM D6 — world data fixes

What was changed, what it does to existing saves, what was verified, and how to integrate it. The
findings and measurements are in [`APFM_D6_WORLD_DATA_AUDIT.md`](APFM_D6_WORLD_DATA_AUDIT.md).

## 1. Commits

Branch `deepseek/d6-world-data`, base `e5282b6749a03503e6a1183df03705472391cae4`, not pushed and
not merged. Each commit is a validated unit:

| Commit | Subject | Files |
| --- | --- | --- |
| `da495676` | `fix(world): re-derive a player's rating when his position is refined` | `player_identity.rs`; the three generator call sites; `save_manager.rs` test fixture; `market_fixtures/mod.rs`; `generated_world_integrity.rs`; the reload fixtures (`pending_transfer_reload_tests`, `run3_integration_tests`); the shared-fixture `#[expect(dead_code)]` lines |
| `71f0ff70` | `feat(audit): check contracts, club ids and competition references` | `world_integrity.rs`; new `tests/d6_world_measure.rs`; new `tests/world_competition_integrity.rs` |
| this commit | `docs(world-data): record the D6 audit and fixes` | `docs/APFM_D6_WORLD_DATA_AUDIT.md`, `docs/APFM_D6_WORLD_DATA_FIXES.md`, and a provenance cross-reference |

No frontend, no database schema, no save format, no migration, no tactics, no transfer decision,
no financial rule was touched.

## 2. The fix

### 2.1 Re-derive the rating when the identity changes

`player_identity::upgrade_player_identity` now takes the date being audited (`on: NaiveDate`) and,
when it actually changes `natural_position`, alternate positions, footedness or weak foot, calls
`player_rating::refresh_player_derived(player, on)`. That recomputes `ovr` and the
position-dependent traits from the corrected natural position, which is what every other writer
already does.

Callers updated: `career::begin_career` (through `upgrade_game_player_identities`,
which now passes the game clock date), `save_manager` load, `legacy_migration`, and the generator
paths that mint a specific-position player (`generate_national_team_player`, youth intake, and a
generator test).

`potential` is **not** shifted with the rating. It is a ceiling on the same scale, so the minimal
repair is to keep it and let `refresh_player_derived` raise it only when the corrected `ovr` would
overtake it (it can never sit below `ovr`). Measured: 2,057 ceilings were raised to the corrected
`ovr`, 300 of them for players aged 23 or under, largest raise 22; 1,939 players (1,740 aged ≤ 23)
see growth room shrink by exactly the size of the rating correction. Shifting every ceiling by the
rating delta was tried and rejected: it rewrote 3,608 ceilings (a balance change) and changed
delegated-recruitment trajectories enough to expose an unrelated latent budget gap (§5).

The function is idempotent: `needs_identity_upgrade` only fires while a coarse bucket remains, so a
save that has already been repaired is a no-op on later loads.

### 2.2 Fixtures made rating-consistent

The reload fixtures set `ovr = 86`/`70` on the shared 60-level attribute block. That stored exactly
the inconsistency the fix repairs, so a reloaded career's player was legitimately re-rated while the
never-saved career's was not — the test was comparing two different players. `market_fixtures` now
offers `rated_player(date_of_birth, level)`, whose flat attribute block rates at exactly `level`
whichever granular role is inferred. The save-manager wonderkid-on-load fixture derives `ovr` from
its attributes and sets the ceiling relative to it. No production code was relaxed to keep a fixture
passing.

## 3. The audit additions

`world_integrity` gained contract checks (inside `audit_players`) and
`audit_competitions(competitions, known_team_ids)` plus the codes listed in the audit doc §3.
`audit_players`' signature and report shape are unchanged; both APIs are pure, read-only, and
stable in ordering. `d6_world_measure` is the reproducible probe (ignored by default);
`world_competition_integrity` is the real-career regression that audits 45 competitions and 9,021
fixtures with `nt-*` national teams in the known set.

## 4. Save impact

| Case | Behaviour |
| --- | --- |
| **New career** | Players are rated correctly from the moment `begin_career` refines their position; nothing to migrate. |
| **Save whose players still hold coarse positions** (pre-fix saves, legacy imports) | On load, the existing identity upgrade refines the position and now also refreshes `ovr`/traits. `save_manager` already sets `needs_resave` when identities change, so the repaired state is written back without a format bump. |
| **Save whose players already hold granular positions but a stale `ovr`** (written after a pre-fix opening) | Not repaired automatically: no marker distinguishes "stale because of the old bug" from an authored value, and a blanket rewrite would need a save migration. Deferred with a proposed migration in the audit doc §6. |
| **Schema, ids, contracts, money, competitions** | Unchanged. No table, column or serde field was added or removed; no player id was reassigned; no transfer, wage or fixture was rewritten. |
| **Potential** | Preserved; raised only where the corrected rating exceeded the stored ceiling. `potential >= ovr` holds for every player. |
| **Frontend** | Unaffected; it reads the same fields. |

## 5. Verification

Run in this worktree on rustc/cargo 1.95.0, macOS arm64.

| Check | Result |
| --- | --- |
| `cargo test --workspace` | **4,733 passed, 0 failed, 77 ignored**, 124 targets |
| `cargo clippy --workspace --all-targets` | clean after touching the changed sources (no warnings) |
| `cargo fmt --all -- --check` | clean |
| `cargo test -p ofm_core` | all targets green, including doc-tests |
| `cargo test -p db` | green (save/load, reload, pending-transfer and legacy-migration suites) |

Named regression evidence:

| Test | What it pins |
| --- | --- |
| `player_identity::tests::upgrading_a_legacy_bucket_refreshes_the_derived_rating` | a coarse→granular change re-derives `ovr`; the ceiling is kept unless overtaken |
| `generated_world_integrity::identifying_generated_players_keeps_their_ratings_consistent` | the shipped world passes `audit_players` **after** identification (this is the test that fails without the fix: 3,786 errors) |
| `world_competition_integrity::a_generated_career_competition_graph_is_referentially_sound` | 45 competitions / 9,021 fixtures, national teams accepted, zero findings |
| `world_integrity::tests::a_repeated_team_id_is_reported_once` | a duplicated club id is one finding |
| `world_integrity::tests::contract_defects_are_reported` | reversed/invalid contract = error, expiry = warning |
| `world_integrity::tests::legitimate_contract_states_are_not_reported` | a missing start (pre-career) and a free agent are not corruption |
| `world_integrity::tests::a_consistent_competition_has_no_findings` | no false positive on a clean graph |
| `world_integrity::tests::competition_and_fixture_defects_are_reported` | every new competition code fires; a replay is only a warning |
| `db::pending_transfer_reload_tests` (3) | reload equals never-saved on wages, dates, money and movement history |
| `season_harness::delegated_recruitment` (2) | full delegation stays inside the club's books on the corrected world (the transfer policy was not changed) |

The 77 ignored tests are measurement studies (`#[ignore]`), now including `d6_world_measure`, which
was run explicitly with `--ignored --nocapture` for the figures in the audit doc.

**Not run:** `npm test` and `npm run build`. No frontend file was changed and this worktree has no
`node_modules`; the Rust workspace, clippy and formatting are the relevant gates for this change.
**Not verified:** FM24 `world.db` behaviour — the file is not in this worktree (see licensing).

## 6. Licensing

- Every player, club and competition in the audited world is generated and fictional. D6 adds no
  real-player record, no career history and no new field of personal data.
- No data was scraped, downloaded or copied. FM24's commercial database is the user's own file; it
  is never in this repository and was not available here. The importer was not modified.
- The repository's distribution guard and `NOTICE-APFM.md` are untouched.
- The audit reports counts and codes, not a licensed dataset. Running it on an imported world in
  someone's own installation is local use of their own file.

## 7. Integrating with Claude's playable-career branch

The branch is cut from the current P4 baseline (`e5282b67`), which is the same tree Claude's
worktree was consolidated on. Two integration paths, in order of preference:

```bash
# In Claude's worktree, after fetching this branch (do not force anything):
git merge --no-ff deepseek/d6-world-data
# or, if a single squashed review is preferred:
git cherry-pick da495676 71f0ff70   # then bring the two docs across
```

Then verify with the commands in §5. Expect **no conflict** in frontend, pages, services, i18n or
the Tauri command layer: the change is confined to `ofm_core`, one `db` test fixture and tests.
If Claude has meanwhile touched these files, resolve in this order:

1. **`ofm_core/src/player_identity.rs`** — keep Claude's identity-inference edits and keep the new
   `on: NaiveDate` parameter plus the `refresh_player_derived` call when `changed`. The call is the
   whole fix; do not drop it while resolving.
2. **`ofm_core/src/generator/*`** — the three call sites only gained the date argument.
3. **`ofm_core/src/world_integrity.rs`** — the additions are new enum variants, match arms and a new
   function. If both sides added variants, keep both and re-run `cargo test -p ofm_core --lib
   world_integrity` so every `code()`/`severity()` arm is exhaustive.
4. **`db/src/save_manager.rs`** — only the `wonderkid` test fixture changed; keep the load-path call
   to `upgrade_game_player_identities` untouched.
5. **`market_fixtures/mod.rs` and the `#[expect(dead_code)]` lines** — keep the helpers; the expect
   line is the repository's shared-fixture convention.

Rules for the integration: do not move the refresh call into the generator only (old saves would
stay wrong), do not add an attribute rewrite (D6 does not rebalance attributes), do not lower the
drift tolerance to make a failure disappear, and do not force-push either side.

## 8. Verdict

**GO for integration**, with two carry-overs and no blockers:

- New careers and pre-fix saves with coarse positions are repaired and audited clean.
- A save written after a pre-fix opening (granular positions, stale `ovr`) is not retroactively
  repaired; it is documented, measurable with the new probe/audit, and needs a dedicated migration
  if the project wants it.
- Market values and wages still reflect the pre-correction coarse rating; that is a balance decision
  for a separate change, not a data-integrity blocker.

The change was AI-assisted (DeepSeek, D6 worktree). No check above is claimed as passing beyond what
is recorded here, and nothing was pushed or merged.
