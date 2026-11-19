# APFM I5 + I9 integration and validation

Integration of `deepseek/i5-tactics` (tactical intelligence) and `claude/i9-staff-delegation` (staff,
delegation and training) into `apfm/main`. Designs: [`APFM_I5_TACTICAL_INTELLIGENCE.md`](APFM_I5_TACTICAL_INTELLIGENCE.md),
[`APFM_I9_STAFF_DELEGATION_TRAINING.md`](APFM_I9_STAFF_DELEGATION_TRAINING.md). Their own evidence:
[`APFM_I5_VALIDATION.md`](APFM_I5_VALIDATION.md), [`APFM_I9_VALIDATION.md`](APFM_I9_VALIDATION.md), both unchanged.

Evidence labels: **RUN** (executed on the merged tree), **READ** (read in source), **NOT RUN**.

## 1. Commits

| | SHA |
| --- | --- |
| Baseline `apfm/main` (I4) | `f31dfa2669f68e0025e3668c8553dd4eb116db2a` |
| I5 head merged | `2b0460df` |
| I9 head merged | `55991a43` |
| **I5 merge commit** | `c7bf678c1874faf37ac5fbde5d03d85cdeee4c1a` |
| **I9 merge commit** | `4fa2aa9825d53d098720cab3806513120987fbe5` |
| Backup branch of the baseline | `backup/apfm-main-pre-i5-i9-20261118` |
| Last code commit (all gates in section 4 ran on it) | `eb96aaf9` |

Integration commits after the merges, in order: `96e2dad9` tests for the I5/I9 seam, `4721f99b` the
delegation fix (section 3), `890484c7` strengthened I5 tests, `eb96aaf9` locale corrections. The commit
that adds this document changes documentation only. Both merges were non-fast-forward. The I5 and I9
source worktrees were not modified (a disposable detached worktree of the I9 commit was used for the
comparison in section 6 and removed).

## 2. Conflicts and their resolution

**There were no textual conflicts.** The two branches touch disjoint files (`comm` of the two
`git diff --name-only` lists is empty); the only shared lines are in `Cargo.lock` (one I5 line) and
`ofm_core/src/lib.rs` (I5 only). The semantic contracts were checked by reading the merged tree:

| Contract | Result (READ) |
| --- | --- |
| `ai_training::plan_for_team -> Option<(TrainingFocus, TrainingIntensity)>` | unchanged; I9 changed one visibility (`likely_starters_condition`) |
| `delegation_control::tactics` calls `ai_tactics::plan_for_club` | unchanged; I5 did not touch `ai_tactics` |
| I5 pre-match response never reaches the manager's dugout | `live_match_manager.rs` skips `user_side`, delegated or not (tested, section 5) |
| The response is match-local | it turns dials on the live match's copy; a played match leaves both clubs' saved plans unchanged (tested) |
| I9 does not bypass I5, I5 does not bypass I9 | I5 reads no delegation state; I9's delegated lineup uses `turn::squad::delegated_eleven`, not `formation_fit` |

**What this means for "delegated staff use the final I5 planner".** The delegated *tactics* duty uses
`ai_tactics::plan_for_club`, which I5 did not change, so it uses the same planner as before I5. A
Total-delegated user club does **not** receive I5's opponent-specific pre-match response: that is applied
to AI-managed sides only. That is by I5's design (the manager's dugout is the manager's) and is recorded
here as a product decision, not a defect.

## 3. Integration fixes

| Fix | Why |
| --- | --- |
| `fix(delegation)`: `recommendations_read_by` no longer returns only the vacancy item when a *named reader* is given and the club has no assistant | Found by the architecture review and reproduced (red) before the fix. A head of youth, coach or head of scouting named as owner of Youth, MatchPreparation or Scouting did nothing, while `run_after_day` marked the duty run and the screen showed him in charge. Test: `a_named_head_of_youth_promotes_when_the_club_has_no_assistant`. This is an I9 defect that did not depend on I5. |
| `fix(i18n)`: the injured-players sentence ("1 players are injured" in seven locales), gendered or ungrammatical `training.outlook.setByYou` in es, pt, pt-BR, the zh-CN duration phrasing, and four locales naming the Staff tab differently from the navigation | i18n audit |
| Tests only: pre-match ration test could not fail (its fixture never reached the ration) and the cap test survived a cap of 1; nothing proved the live resolvers apply the depth factor | test review; mutations in section 5 |

No production behaviour was changed to make a gate pass, no test was weakened, and no warning was disabled.

## 4. Validation commands and results (all RUN on the last code commit unless marked)

| Gate | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | exit 0 |
| Clippy, default | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings` | exit 0, no warnings |
| Clippy, `mcp` | the same with `--features mcp` | exit 0, no warnings |
| Rust workspace | `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace` | exit 0; 160 result lines; **5,173 passed, 0 failed, 128 ignored** (I9 alone: 5,139 / 125). Ignored tests are the existing measurement studies, not skipped gates |
| Frontend | `npm test` | 348 files passed, 1 skipped; **3,066 passed, 4 skipped, 0 failed**, exit 0 |
| Build | `npm run build` (`tsc && vite build`) | exit 0 |
| Lint | `npm run lint` (Biome 2.5.5, confirmed the real package) | exit 0, 3 infos |
| Format check | `npm run format:check` | exit 0 |
| Dead code | `npm run knip` | exit 0 (two configuration hints, not findings) |
| i18n | `npm exec --no -- vitest run src/i18n` | 9 files, 35 tests passed |

The frontend gates ran on `eb96aaf9`'s tree. A first attempt to run them in a loop over unquoted
command strings failed with exit 127 under zsh (no word splitting); that log was discarded and every
command was re-run spelled out, as reported above. Earlier, the full Rust workspace run on `96e2dad9`
(before the fixes above) gave 159 result lines, 5,169 passed, 0 failed, 128 ignored; it is superseded by
the run in the table and is mentioned only for the record.

## 5. Combined I5 / I9 regression evidence

New tests (`engine/tests/match_config_compat.rs`, `engine/tests/group_depth_wiring.rs`,
`season-harness/tests/i5_i9_integration.rs`, plus the fix test and the corrected `matchday_tactics`
tests). Existing tests reused where they already cover the behaviour.

| | Scenario | Evidence |
| --- | --- | --- |
| A | Named duty owners survive save/reload | I9 `delegation_repo` round-trip, I9 reload test, and the new `a_total_delegated_career_reloads_into_the_same_formation_and_tactics` (SQLite, fresh reader, replays 40 days from reloaded and never-saved games, identical club plan and delegation state) |
| B | Delegated tactics use the I5-era planner | `in_full_mode_the_assistant_names_the_eleven_and_sets_the_tactics` asserts `tactics_phase == plan_for_club`; the new career test asserts the tactics duty really wrote a plan |
| C | Manual tactics stay under the manager | `the_assistant_does_not_overwrite_the_managers_*`, and the new `the_pre_match_response_never_touches_the_managers_dugout` (home and away, delegated or not) |
| D | Pre-match response persists nothing | new `a_played_match_leaves_both_clubs_saved_plans_as_they_were` |
| E | Training approval windows and delegated training | I9 `delegation_training` tests (13) and the training-only study arms on three seeds (section 6) |
| F | Total delegation keeps the floor and the 30 limit | the career test asserts 15 to 30 seniors on every one of 100 days, both games; study results in section 6 (one transient exception) |
| G | I4A recruitment, I4B foreign calendars | existing `i4_strategic_ai`, `foreign_calendar`, `delegated_recruitment` suites, all green in the full run |
| H | Depth-aware engine active and deterministic | new `group_depth_wiring`: full groups are exactly unaffected by the weight; a lone finishing specialist and a thin elite back line are worth measurably less at weight 1 than weight 0 |
| I | Red-card reactions | I5 `ai_decision_tests` (3), green |
| J | Old saves and configurations | new `a_configuration_written_before_group_depth_loads_with_weight_zero`; I9 `a_delegation_saved_before_owners_loads_with_the_assistant_in_charge` |
| K | M12 preparation, simulation, finalization | existing `p5_formation_persistence`, `adapt_spatial_fixture` and live-match suites, green |
| L | I9 frontend commands and outlook | I9 service, hook and card tests, green in the 3,066 |

**Mutation checks (RUN, then restored).** Removing the human-side skip in `live_match_manager` turns
`the_pre_match_response_never_touches_the_managers_dugout` red. Removing the ration check, and lowering
`MAX_MATCHDAY_MOVES` to 1, each turn `matchday_tactics` tests red. Removing the finishing depth factor in
`zone_resolution`, and passing weight 0 to the live `defensive_quality`, each turn `group_depth_wiring`
red. The first version of the lone-forward test survived removal of the finishing factor (other forward
contests carried the effect), so its fixture was changed to a pure finishing specialist and the
mutation then failed it. **Not run:** the write-back mutation for the played-match test, and the other
mutations the test review proposed (the `delegation_training` decline expiry, the owner rating
threading, the outlook fields, `domain::set_owner`).

## 6. Long-career study (RUN, 10 seasons, generated `gate` world)

Three seeds, every I9 study arm on seed 1 and the manual, training-only and all-delegated arms on seeds
2 and 3. The study asserts at most 30 and at least 15 seniors every day and clean world invariants.

**Passes.** Invariants clean in every arm run. No day above 30 seniors in any arm. Training-only
delegation holds on the merged tree (match-eve condition, acts alone / ask me: seed 2 84.5 / 85.2, seed 3
84.5 / 87.7, against 85.4 and 86.4 manual) and the study's own assertions on those arms pass. Match
counts, injuries (13 to 18 a season) and the age curves are in the same range as I9's report. The I5
career study (2 worlds, 5 seasons) reads, for world 1 season 2, 2.547 goals, 24.89 shots, 7.14 on target
and 51.52 % possession: **identical** to I5's own report, so I9 does not perturb the AI league. Shapes
played: 4-4-2 51.7 % over 948 club-seasons (I5 reported 52.5 %), eight shapes in use, about 21 clubs
changing shape between seasons, 2.63 goals a match, 0.108 red cards a match.

**Performance.** Each I9 arm of ten seasons takes 128 to 133 s, against 127 s on the I9 branch. The I5
career costs 12.5 s a season in the dev profile (I5 measured 10.5 s in release; not comparable, no
release build was made). No obvious regression.

**Finding 1 (not a gate failure): the all-delegated arm is less fit on the merged tree.** Match-eve
condition of the likely starters, arm "training and tactics delegated", same seed and arm:

| Seed | I9 alone | Merged | Merged, `group_depth_weight` 0 |
| --- | --- | --- | --- |
| 1 | 86.2 | 83.8 | 85.8 |
| 2 | 84.7 | 78.0 (29 % of matches below 70, 9 % below 60) | 84.9 |
| 3 | 85.3 | 80.8 | 83.5 |
| mean | 85.4 | 80.9 | 84.7 |

The manual arm does not move systematically (86.7, 87.8, 85.0 alone; 89.7, 85.4, 86.4 merged). With
the weight at 0 the all-delegated arm returns to the I9-alone level on all three seeds; with the
pre-match response removed instead (seed 2 only) it recovers part of the way (81.6). So the I5
depth-aware engine interacts with Total delegation of the **tactics** duty and costs about 4.5 points
of match-eve condition in this world. The mechanism is not established. I9's own report recorded that
`ai_tactics` reads the stamina *attribute*, not current condition, when deciding whether a side can
press, and suggested a re-run of the tactics arm if the tactical side changed: this is that re-run. It
is three seeds of one club in one generated world, with seed-to-seed differences of 2 to 3 points in
this study, so the size is uncertain; the sign is the same on all three seeds. No code was changed in
response: reading condition in the planner would change every AI club's tactics, and disabling the
depth weight would remove I5's central change. **Recommended next step:** a short mission that lets the
delegated tactics duty (and the planner) read current condition, then repeats this arm.

**Finding 2 (not a gate failure): the 15-senior floor is best-effort, and was breached for one day.**
Seed 2, manual arm, merged tree: 14 seniors on the first day of season 6 (a missing forward group slot).
The same arm on the I9 branch alone has 0 breach days, so the trajectories differ, but the keeper is
documented as best-effort ("when no source has anyone, the club stays short"), and on that day the academy
and the affordable free agents had no forward; it was repaired the next day. The study's strict
assertion therefore **fails on this seed in this arm**, and it was left strict. Seeds 1 and 3 and the other
arms had 0 breach days. I9's "0 floor-breach days in all arms" is a statement about seed 1, not a
guarantee.

**Finding 3 (observation): lowest cash.** On seed 3 the weak-staff and no-staff arms reached
-3.50 M and -1.55 M, below the club's starting cash (the manual arm's lowest equals the start). The
harness invariants were clean and no I9 or I5 comparison was made for these two arms, so this is not
attributed.

## 7. Save compatibility

READ and RUN. `MatchConfig::group_depth_weight` is `#[serde(default)]` and reads 0 (the plain mean, the
old engine) when absent, and `Default` gives the shipped 1.0 (new test, both directions). A career builds
its `MatchConfig` from `Default` for every match, so a loaded career plays the new rule. `DutySetting::owner`
is `#[serde(default)]` inside the existing `staff_delegation` JSON row: no migration, no `MIGRATION_COUNT`
change, and a save from before owners loads with the assistant in charge (I9 test). No SQL changed in
either branch. The full SQLite reload of a Total-delegated career, replayed against a never-saved copy,
is the new combined test.

## 8. Known remaining limitations

**I5 (preserved from its report).** The dominant formation moved from 4-3-3 to 4-4-2 (about 52 %, 51.7 %
measured again here). Five-zone and M12 still differ materially on some dials, notably defensive-line
height. The isolated effect of the pre-match response was not measured, and M12's paired study had 60
matches per setting. The legacy instant engine does not use every new group-depth contest factor.

**I9 (preserved from its report).** The ten-season staff study used one seed in I9's report; this
integration added two. Staff wages were zero in the generated world. The Tauri GUI was not manually
tested by I9. Non-English translations were not reviewed by native speakers (the corrections in section
3 are also not native-reviewed). The effect of staff on long-term development was not statistically
established.

**New or sharpened here.** Findings 1 to 3 above. A Total-delegated user club gets no opponent-specific
pre-match response. The architecture review's smaller items were not actioned: `create_live_match_with`
is 142 lines and `recommendations_read_by` 420 (both over the 100-line ceiling and already over before
I5/I9), `StaffManagementPanel` is about 326 lines (limit 200), `delegation_training` exposes `pub` items
only used by one sibling, `matchday_tactics` is `pub mod` with only `pub(crate)` items, and
`season-harness` gained a dev-dependency on `engine` that the CLAUDE.md crate graph does not classify
(a human should classify it; it points toward `engine` and does not violate engine isolation).

## 9. Reviewer findings

All five reviewers were run as the repository's own agents, read-only, with a note not to start builds.
Their reports are model output; the findings below are the ones I verified or acted on.

| Reviewer | Outcome |
| --- | --- |
| `ofm-architecture-reviewer` | No engine-isolation, layering, save-compatibility or command-size violation. One reachable I9 defect (F1), **fixed**. Size, visibility and module-cycle findings listed in section 8, not actioned. |
| `i18n-auditor` | 30 keys present in 12 locales, placeholders match, Rust emits keys. Translation-quality findings: "1 players", gendered and ungrammatical strings, nav terms: **fixed**. Not actioned: signed percentages and hard-coded `%` (advisory). |
| `ui-accessibility-reviewer` | No blocking findings. Not actioned (frontend behaviour left unchanged): focus is lost after choosing a duty owner because the control is disabled while busy (the same pre-existing pattern as the neighbouring controls), a blank selector if a duty has several candidates and no owner, a dead key `staff.delegation.owner.label`, and `useTrainingOutlook` failing silently. No browser or screen reader was used. |
| `ofm-dedup-reviewer` | No duplication between I5 and I9. In-branch duplicates not actioned: the taper-to-intensity rule restated in `training/outlook.rs`, the congested-week threshold decided in four places (`BUSY_WEEK` 3 against the shared 2), starters' condition computed three ways, per-player group formulas mirrored between the engine and `formation_fit`, two owner view structs. |
| `ofm-test-reviewer` | Three HIGH findings: (1) the live resolvers' depth wiring untested, **fixed**; (2) the match-day ration test could not fail, **fixed**; (3) the `formation_fit` "same rule as the engine" tests are tautologies and no test pins parity between the engine's per-player formulas and the mirrored ones, **not actioned**. Many MEDIUM findings (outlook fields, decline window expiry, owner rating threading, boundary cases) not actioned. |

I did not run `/preflight` as a single skill; I ran its components (fmt, both Clippy passes, the full Rust
and frontend suites, build, lint, format, knip, i18n) and all five reviewers as listed.

## 10. Desktop application

**Automated coverage, launch smoke and manual testing are three different things here.**

- *Automated coverage:* the frontend suites for the I9 screens (Staff Management owner selector,
  Training Outlook card, hook and services) pass, section 4.
- *Launch smoke (RUN):* `npm run tauri dev` built the real desktop crate (`openfootmanager`, dev
  profile) and started it. The webview loaded from the Vite server and logged
  `AP Football Manager v0.3.0-dev - eb96aaf`; the backend answered `get_settings` and `get_active_game`;
  the process stayed up and the log contains no panic, `[ERROR]` or `[WARN]` line. It was then stopped.
- *Manual GUI testing: NOT DONE.* Nobody opened a career, the Staff tab, the duty-owner selector, the
  Training tab or the outlook card, played a match, or saved and reloaded in the running app. The
  claims about those screens rest on the automated tests only. The reviewers' accessibility findings
  (section 9) are likewise unverified in a browser or with a screen reader.

## 11. Release blockers

None that fail a mandatory gate. Findings 1 and 2 in section 6 are quality regressions in specific
configurations (Total delegation of tactics; a single day below the floor) and should be read before a
release build. They were deliberately not "fixed" here: the first needs a design decision (section 6),
the second is the documented contract of a best-effort keeper.

## 12. Readiness for the next missions

The tree builds, formats, lints and passes every mechanical gate, the I5/I9 seam is held by tests that
fail when the seam is broken, and save compatibility is preserved. APFM is ready for further
development, with two carry-overs: (1) make the delegated tactics duty read current condition and re-run
the all-delegated arm on three or more seeds; (2) decide whether the squad floor under Total delegation
should be a hard guarantee (it would then have to create or borrow a player, which the game never does) or
stay best-effort with the study assertion relaxed deliberately and visibly. A manual GUI pass of the Staff
and Training screens is still owed.
