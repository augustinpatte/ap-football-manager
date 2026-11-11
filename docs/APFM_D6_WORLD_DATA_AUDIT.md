# APFM D6 — world data audit

Audit of the initial world's data integrity: clubs, players, contracts, ratings and the competition
graph. Findings and evidence live here; the code changes, save impact, test results and the
integration verdict are in [`APFM_D6_WORLD_DATA_FIXES.md`](APFM_D6_WORLD_DATA_FIXES.md).

## 1. Baseline and scope

| | |
| --- | --- |
| Branch | `deepseek/d6-world-data` (off `develop`) |
| Base commit | `e5282b6749a03503e6a1183df03705472391cae4` — *docs(playable-career): P4 integration baseline after merging F5.5* |
| Toolchain | rustc/cargo 1.95.0, macOS arm64 |
| Worktree | `/Users/augustinpatte/apfm-migration/apfm-deepseek-d6` |
| Merged/pushed | No. Nothing was merged into `apfm/main`, pushed, or reset. |

The D6 question was whether the world the game ships is internally consistent, and whether the
audit tools would catch real corruption without rejecting legitimate football situations. The
answer is in §4–§6.

Not touched: AI tactics and manager behaviour, Squad Intelligence, transfer decision algorithms,
financial simulation, season rollover, M12, the 2D viewer, the React frontend, save migrations, the
database schema. No real-player record, career history or licensed datum was added; nothing was
scraped.

## 2. What the shipped world is, and where it comes from

The repository does not contain FM24's `world.db`. The world a new career starts from is the
procedural generator, so that is the subject of the audit. Import findings are read from the
importer's code and its tests, not from the commercial file (which was not available).

| Source | Content | Real people? | Audit coverage |
| --- | --- | --- | --- |
| **Random World** (`ofm_core::generator`, the default) | 440 generated clubs × 23 players (19 senior + 4 academy) + 1,760 free agents = **11,880 players** | No — entirely fictional | Measured directly on seed `20260820` (§4) |
| `.ofm` packages / world JSON | Whatever the author wrote; the repository ships only fictional examples | Only if the author made them so | Same audit runs on any loaded world; not run on an author file here |
| FM24 import (`crates/fm24-adapter`) | The user's own read-only save; ids `fm-p-<person.id>`; short squads topped up with generated fillers | The people in the user's file | Code + tests only; `world.db` absent, so **not** data-audited |

Provenance and licensing are unchanged from [`APFM_DATA_PROVENANCE.md`](APFM_DATA_PROVENANCE.md).
That document's claim that stored `ovr` and the attribute-derived rating differ by at most 0.5 is
true **before** the career-start identity upgrade, which is exactly the state it measured. D6 shows
it stopped being true immediately after that upgrade; §5.1 is that defect and it is now fixed, so
the claim holds in both states.

### 2.1 The pipeline, and the seam where ratings diverged

1. **Generation** (`generator::generate_world_seeded`) builds each player with a coarse group
   position (`Defender`, `Midfielder`, `Forward`) and derives `ovr`, `potential`, market value and
   wage from that group. A coarse group is canonicalised before scoring (`Defender` → CenterBack,
   `Midfielder` → CentralMidfielder, `Forward` → Striker).
2. **Opening** (`career::begin_career`) repairs youth academies, stamps contract starts, and calls
   `player_identity::upgrade_game_player_identities`, which replaces the coarse group with a
   granular natural position (LeftBack, RightWinger, …).
3. **Load** (`db::save_manager`) runs the same identity upgrade on every save, and resaves when it
   changed something. `db::legacy_migration` does the same for a pre-ledger save.

Step 2 is the seam: `natural_position` is an input to `player_rating::natural_ovr`, so changing it
without re-deriving the rating left `ovr` describing the role the player no longer has. Training
(`development.rs`) and every other writer recompute `ovr` from attributes, so a stored `ovr` more
than the audit tolerance from its attributes is corruption, not form.

## 3. The integrity checks

`ofm_core::world_integrity` is pure and read-only, with two severities. `audit_players(teams,
players, limits)` is unchanged in shape; `audit_competitions(competitions, known_team_ids)` is new.
`known_team_ids` is the union of club and national-team ids, because the World Cup names national
teams that are not in `teams`; omitting them would be a false positive.

| Code | Severity | Catches | Legitimate situation that must not be flagged |
| --- | --- | --- | --- |
| `duplicate_player_id` | Error | one id on two players | — |
| `duplicate_identity` | Error | same full name + birth date + nationality | two different people sharing a name (different dates/ids) are allowed |
| `duplicate_team_id` | Error | two clubs with one id | — |
| `rating_out_of_range` | Error | `ovr`/`potential` outside 1–99 | — |
| `potential_below_overall` | Error | ceiling below current ability | — |
| `attribute_out_of_range` | Error | an attribute above 100 | — |
| `ovr_disagrees_with_attributes` | Error | stored `ovr` > 2.0 from the position-weighted attribute rating | rounding up to 0.5 |
| `invalid_birth_date` | Error | unparsable date of birth | — |
| `implausible_age` | Warning | age outside 14–45 | — |
| `unknown_club` | Error | `team_id` naming no club | a free agent (`team_id = None`) |
| `unknown_parent_club` | Error | loan parent naming no club | a loan to a club outside the world (named borrower) |
| `loan_club_mismatch` | Error | `team_id` ≠ loan club | — |
| `retired_on_roster` | Error | a retired player still on a club | retired and unattached |
| `unknown_lineup_player` | Error | starting XI naming no player | — |
| `too_few_seniors` | Error | fewer than the senior floor on a club's books | — |
| `squad_too_large` | Warning | more than the roster ceiling | — |
| `contract_ends_before_start` | Error | a staged contract whose end is not after its start | — |
| `contract_date_invalid` | Error | an unparsable contract start or end | — |
| `contract_expired_at_audit_date` | Warning | contracted player whose deal ended before the audit date | a save taken mid-negotiation |
| `competition_without_participants` | Warning | a competition with no participants | a progressively drawn cup must still name its entrants |
| `unknown_competition_participant` | Error | a participant the world does not contain | national teams, when passed in `known_team_ids` |
| `fixture_unknown_team` | Error | a fixture naming an unknown team | — |
| `fixture_self_match` | Error | a team paired with itself | — |
| `duplicate_fixture_id` | Error | one fixture id in two places | — |
| `fixture_team_not_participant` | Error | a fixture team outside the competition's participants | — |
| `duplicate_fixture_pair` | Warning | the same home/away pairing twice in one competition | a cup replay |

Contract checks only apply when both dates are present. A missing start is the legitimate generated
state before a career opens (the start is stamped at opening); a missing end is the legitimate free
agent / undated state. Both are covered by a no-false-positive test.

```bash
# Rating, contract and roster audit of the generated world, plus the competition graph.
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --test d6_world_measure -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --test generated_world_integrity
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --test world_competition_integrity
cargo test --manifest-path src-tauri/Cargo.toml -p ofm_core --lib world_integrity
```

The probe is `#[ignore]`d so it never slows the suite; it prints the pre-identification,
post-identification and post-`begin_career` states, the drift histogram, the contract counts and the
competition findings.

## 4. Measurements

All figures: seed `20260820`, opening year 2026, game date 2026-07-01, generated world.

### 4.1 World shape

| Check | Count |
| --- | --- |
| Clubs | 440 |
| Players | 11,880 (10,120 on a club, 1,760 free agents) |
| Roster rows / club | 10,120 rows, min 23, max 23, mean 23.00, empty clubs 0 |
| Clubs with fewer than 3 goalkeepers | 0 |
| Implausible ages | 0 (bands: 2,540 aged 16–18; 3,870 19–23; 2,813 24–29; 2,458 30–34; 199 35–39) |
| Duplicate player / club ids, duplicate identities | 0 (identity uniqueness was fixed in the earlier provenance run) |
| Contract defects before opening | 0 (start deliberately missing on all 10,120) |
| Contract defects after opening | 0 (10,120 contracts have start and end; 0 expired, 0 invalid, 0 reversed) |
| Competitions after `begin_career` | 45 |
| Fixtures | 9,021 |
| Unknown participants | 0, once the 48 `nt-*` World Cup national teams are in the known set (0 teams in no competition) |
| Unknown / self / duplicate fixtures, fixture teams outside participants | 0 each |

Cups schedule their later rounds progressively, so a cup with 20 participants and 4 fixtures is not
a defect; the audit checks references and pairings, not completeness of the calendar.

### 4.2 Rating integrity — before the fix

| State | `ovr_disagrees_with_attributes` | Drift > 0.5 | > 1.0 | > 2.0 | Max |
| --- | --- | --- | --- | --- | --- |
| Generated, before identification | 0 | 0.0% | 0.0% | 0 | 0.50 |
| Generated, after identification | **3,786** | 4,638 (39.0%) | 4,361 (36.7%) | 3,786 (31.9%) | **26.21** |
| After `begin_career` | **3,817** | 4,686 (39.1%) | 4,403 (36.7%) | 3,817 | 26.21 |

The drift is the identity upgrade rewriting the position under a rating that had already been
computed from the old one. It is not random noise: it is one-directional for pace-heavy roles
(a winger scores higher than a generic forward) and it pushed players across tier and wage
thresholds.

### 4.3 Rating integrity — after the fix

| State | Findings | Drift > 0.5 | > 1.0 | > 2.0 | Max |
| --- | --- | --- | --- | --- | --- |
| Generated, before identification | `{}` | 0.0% | 0.0% | 0 | 0.50 |
| Generated, after identification | `{}` | 0.0% | 0.0% | 0 | 0.50 |
| After `begin_career` | `{}` | 0.0% | 0.0% | 0 | 0.50 |

The remaining 0.50 is round-to-nearest. Contract and competition audits are `{}` in every state.
Ceilings: 2,057 players' stored ceiling was below their corrected rating and was raised to it (300
aged 23 or under; largest raise 22, for a player whose coarse rating was far below his granular one).
`potential >= ovr` holds for all 11,880. Growth room (`potential - ovr`) shrank for 1,939 players
(1,740 aged 23 or under) by exactly the size of their rating correction, because the ceiling was
kept rather than shifted; see the deferred items.

## 5. Confirmed defects

### 5.1 The career-start identity upgrade left `ovr` describing the old position

**Severity: high.** 3,786 audit errors on the shipped world (31.9%), up to 26.21 points of drift,
39.0% of players off by more than 0.5.

**Evidence.** The probe's post-identification block, and the audit error
`ovr_disagrees_with_attributes`. Root cause traced to `upgrade_player_identity`: it replaced the
coarse `natural_position` with a granular role but returned without re-deriving `ovr` or the
position-dependent traits. Training and `refresh_player_derived` always keep the two together, so
this was the only writer that broke the contract.

**Fixed** by re-deriving the derived fields when (and only when) the identity actually changes,
with the career date supplied by the caller. Regression tests:
`player_identity::tests::upgrading_a_legacy_bucket_refreshes_the_derived_rating` and
`generated_world_integrity::identifying_generated_players_keeps_their_ratings_consistent`.

### 5.2 Repository test fixtures encoded the same inconsistency

**Severity: medium (repository correctness, not player-visible).** The reload fixtures gave a
player `ovr = 86` (or 70) while his attributes were still the 60-level block, and the save-manager
wonderkid fixture did the same. The old loader never re-derived, so the contradiction was invisible;
once load repairs ratings, those fixtures described a state the audit calls corruption and the
reloaded career legitimately differed from the never-saved one.

**Fixed** by making fixtures rating-consistent at the level they ask for (`rated_player`), and by
deriving the wonderkid fixture's `ovr` from its attributes. No production behaviour was changed to
accommodate a fixture.

### 5.3 A pre-existing delegation wage-budget gap (found, not fixed here)

**Severity: medium; outside D6's allowed changes.** While validating the fix, the season harness
`total_delegation_manages_multiple_transfers_within_live_finances` exposed a latent gap: the
assistant authorises each signing against the wage bill *as it stands*, but not against wages of
deals already agreed and awaiting registration. With room 28,120 it opened approaches at 18,000 and
16,000; both registered, taking the bill from 319,789 to 337,789 against a 331,909 budget (+1.8%).

This is a transfer-decision/finance behaviour, not world data, and D6 was instructed not to change
those algorithms, so it is reported here rather than patched. It is latent on the base commit too —
the base trajectory simply did not reach it. It is no longer triggered by the corrected world
because ceilings are no longer shifted (an intermediate version that raised every ceiling by the
rating delta did trigger it). The harness is green again. Recommendation for the owner of the
transfer policy: reserve the wages of `PendingRegistration` offers in `books_of`, or re-check the
bill inside `process_pending_transfer_registrations` before executing.

## 6. Deferred findings and residual risk

| Item | Why deferred | Suggested targeted fix |
| --- | --- | --- |
| **Old careers whose players already hold granular positions keep a stale `ovr`.** The load-time identity refresh only fires when the identity is coarse, which is the state of a pre-fix save with legacy positions. A save written *after* a pre-fix opening already has granular positions and the wrong rating. | Repairing every loaded player would need a save migration, explicitly out of D6 scope, and it would override authored `.ofm` worlds that set `ovr` deliberately. | A one-shot migration that re-derives `ovr` only for saves whose fingerprint/format predates the fix, behind the existing `needs_resave` mechanism; do not apply it to imported worlds. |
| **Market value and wage still follow the coarse rating.** Generation sizes both from `ovr_from_attributes` on the group position, before identification; the corrected `ovr` can now sit a few points away from the value that was priced. | Re-pricing is a balance/economy change; D6 must not rebalance. | Move value/wage sizing after identification for future generated worlds, and calibrate against the economy report in one dedicated change. |
| **Growth room shrank for 1,939 players** (1,740 aged ≤ 23) by the rating correction, and 2,057 ceilings were raised to the corrected `ovr`. | Keeping the stored ceiling is the minimal repair; shifting thousands of ceilings is a rebalance. | If the generator's intent is headroom rather than an absolute ceiling, recompute ceilings at generation time after the identity is known, in a dedicated balance change. |
| **Attributes are not rebalanced.** A winger's pace-heavy profile now scores higher than the generic forward weighting did. | Explicitly forbidden in D6. | None proposed; the correction is the point. |
| **No FM24 data audit.** The commercial `world.db` is absent, so PSG-sized squads, missing players and regen identities remain unverified. | File unavailable; D6 forbids scraping. | Run `apfm-import` on the user's machine and read `integrity.world_audit`; new contract/competition checks are available for that report. |
| **Name-pool collisions and name/nationality fallbacks.** Already documented in the provenance report; different people, not corrupt. | Data authoring, not an integrity bug. | Larger name pools. |

## 7. What the checks would and would not catch

- **Caught:** a player whose stored rating no longer matches the attributes the game will show; a
  contract that predates itself; a competition pointing at a club that does not exist; a fixture
  between a club and itself; the same fixture id twice; a team playing in a competition it is not
  entered in.
- **Deliberately tolerant:** rounding up to 0.5; a missing contract start before a career opens; a
  free agent with no contract; a warning (not an error) for an expired deal mid-negotiation; a
  warning (not an error) for a repeated pairing, because a replay is football.
- **Not caught by design:** a name that also exists in real life (the world is fictional and name
  pools overlap), a squad that is legal but unrealistic beyond the roster ceiling, a cup whose later
  rounds are not drawn yet.

Licensing: the audit reads and reports on data; it does not add, copy or redistribute any. FM24 data
remains the user's own commercial file and was never present in this worktree.
