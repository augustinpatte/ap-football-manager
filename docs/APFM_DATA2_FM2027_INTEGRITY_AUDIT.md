# APFM DATA2: FM 2026/27 database integrity, valuations, squads, loans and player histories

Mission DATA2. This document separates **verified source facts** (read from the owner's `world.db`,
read-only), **generated data** (what the importer and the game produce from it) and **assumptions**
(what could not be verified). Nothing in it is a claim about the real macOS application: it was not
operated (no display here).

No football data, save, export or absolute personal path is in this repository. Players are named only
where the mission asked for the six probes; the ranked anomaly list is anonymised.

## 1. Commits

| | |
|---|---|
| Baseline | `ea177fb9` (`apfm/main`, `docs(ux1): ...`) |
| Implementation | `48cc8e17` valuation · `0d53f8c1` squad status evidence and all-club check · `44c6c2a2` provenance of loan terms and earlier careers (+ migration v071, world revision 7) · `ce7882b5` loan and career UI · `a1a2c3a7` reviewer fixes · `7a4dc5a3` test hardening |
| Final | the commit that adds this file (the SHA is reported with the push) |

Code under the full-workspace test runs below: `a1a2c3a7`. The only production change since is one
generated-report sentence; everything else after it is tests (rerun, see §10).

## 2. Source integrity

| | SHA-256 of the owner's `world.db` |
|---|---|
| Before the mission | `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` |
| After the mission | `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` |

Identical. The adapter opens it `SQLITE_OPEN_READ_ONLY` with `PRAGMA query_only`; my own inspection used
`mode=ro&immutable=1`. The importer also hashes it before and after every run (`source.untouched` check).
Generated outputs (worlds, profiles, a scratch CSV of perimeter features) lived in a session scratch
directory and are not committed.

## 3. Findings for the six named players

Players are identified by **source person id**, never by name (the source has two Mbappés and two
Tielemans; the correct rows were chosen by id and by club/age/CA). Values in EUR at the configured fixed
rate of 1.17 per GBP.

| Player (person id) | Verified from the source | Cause of the observation | Final value |
|---|---|---|---|
| Marcus Rashford (12275) | CA 161 / PA 167, age 28, Manchester United, contract to 2028-06-30, stored value 140.33 M GBP (`value_source = save`) | The owner's 164.2 M is the **stored value converted 1:1** (140.33 M × 1.17). The importer did nothing wrong in converting; the save's field is itself high for his ability: among CA 157–165 players aged 26–31 the stored values span 29 M–229 M GBP (median 90 M). | 164.19 M → **121.00 M** (pulled to the edge of the trusted band; model estimate 87 M) |
| Kylian Mbappé (40368) | CA 191 / PA 199, age 27, Real Madrid, contract to 2029-06-30, stored 119.06 M GBP | Same: converted stored value (139.3 M). Stored value sits *below* the median of his own comparable group (CA 185+: 70–229 M GBP, median 134 M). | 139.30 M → **137.02 M** (only the affordability ceiling's bend) |
| Kobbie Mainoo (55635) | CA 154 / PA 174, age 21, Manchester United, contract to 2031-06-30, stored 102.6 M GBP | Converted stored value (120.0 M). | **120.04 M**, unchanged (inside the trusted band) |
| Youri Tielemans (6606) | CA 147 / PA 177, age 29, Manchester United, contract 2026-07-13 to 2031-06-30, stored 51.3 M GBP | Converted stored value (60.0 M); comparable players (CA 144–150, age 27–31) have a median of 34 M GBP. | 60.02 M → **53.50 M** |
| Mika Godts (53728) | CA 106 / PA 161, born 2005-06-07, owner and registration both PSG. **Registered with PSG's team 1399: slot 1, `is_youth = 1`**; squad status code 11; contract 2026-07-13 to 2031-06-30 | **Source registration, not the importer.** His registered side is a youth team (30 registered, median age 19.3); status 11 is carried by 26 of those 30; his CA (106) is below the CA of every one of PSG's 20 first-team registrations (minimum 127). The importer reads the registration, so he is an academy player. | role **Youth** (unchanged); value 17.66 M → **6.97 M** |
| Gabriel Moscardo (63609) | CA 123 / PA 140, born 2005-09-28, **contract club PSG (498)**, registered with **R.C.D. Espanyol de Barcelona (1107), slot 0**; one contract row only; wage £10,947/wk paid under PSG's contract | The save records *where he plays* but **no loan end, wage share or buy option**: the loan is inferred from registration ≠ contract club, and every term shown was configured (0% wage share, end 2027-06-30) yet displayed as fact; the banner also named only the club "other" than the viewer's. | value **3.79 M** (kept); loan now shown *Contract owner PSG → Playing for Espanyol*, terms marked estimated |

**On Godts specifically.** If the owner knows him to be in PSG's first team, the save disagrees on every
signal it carries (registration, status, ability relative to the first team). No override was written; the
brief asked for a policy, not a name. That policy exists (§5) and changes nobody on this save.

## 4. Valuation

### Causes (verified)

* `pipeline.rs` passed the stored value through `cfg.to_eur(value_gbp)`: the observed numbers are the
  save's own, multiplied by 1.17. 9,791 of the 10,045 placed players carry a stored value; 209 carry one
  the source importer generated from its own regression (`générée`); 45 carry none.
* The stored values are noisy as a measure of the player. A log-linear model over ability, age, ceiling,
  contract, club reputation and position explains 90% of their variance, but the residual spread is
  large (median absolute residual **0.44 in log units**): comparable players differ by factors of 2–8.
* Three places turned the source value into a player value (placed players, the free-agent pool, the
  external registry) with three rules; the registry's fallback duplicated the game's projection formula.
* Value scale: the game's own `projected_market_value` gives about €14 M for a 95-rated 27-year-old; the
  imported values are on FM's scale (about ×10). **I preserved FM's scale** (the economy was calibrated on
  it) and corrected only relative inconsistencies. This inconsistency in scale is pre-existing and is listed
  as open (§11).

### Model (`valuation.rs`, parameters in `adapter.toml [valuation]`)

* **Estimate**: log-linear model fitted at import time on the world's own stored values (ability curve with
  knots at 100/130/160, age hinges at 22/27/31, ceiling gap by three age windows, contract years left,
  club reputation, position group), solved by normal equations with a fixed elimination, after one 2-σ
  trim. Nothing about a player is written into the code.
* **Reliability of the source value**: a stored value is kept **to the euro** while within +0.75 / −1.5
  median absolute residuals of the estimate; beyond that it is pulled to the band's edge. The band is
  narrower above (an inflated value is what produces an unbuyable asking price; the estimate is least
  reliable for stars and prospects, where it would raise a value). A value the save generated itself is
  trusted at ±0.25; a missing value takes the estimate.
* **Affordability**: no value exceeds the richest club's transfer budget (£155 M = €181.35 M), squeezed
  rationally above 70% of it so stars stay distinct and ordered.
* The imported world, the free-agent pool and the external registry share the one model, population and
  contract-end rule. A real-source test checks that the registry and the world give **the same value for
  every compared player**.

Three different amounts stay different, as before: the *estimated value* (this module), the *asking price*
(`transfers::realism::asking_fee`: status, contract, window phase) and the *agreed fee* (`agreed_fee`,
haggling). Nothing here makes them equal.

### Results (10,045 placed club players; EUR)

Provenance: 5,282 kept exactly · 4,666 pulled to the band · 52 bent by the ceiling · 45 estimated (no
stored value). Rank-inversion metric: of **97,626** clearly-ordered comparable pairs (same group, within 2
years and 1,500 reputation points, ≥15 CA apart, ceiling not lower by more than 5), **2,253 (2.31%) were
inverted in the stored values and 367 (0.38%) after valuation.**

| group | n | p50 before → after | p90 | p99 | max |
|---|---|---|---|---|---|
| all | 10,045 | 400k → 380k | 13.56M → 10.80M | 105.29M → 92.14M | 335.62M → 166.22M |

By OVR band (p50 / p90 / max, before → after):

| OVR | n | p50 | p90 | max |
|---|---|---|---|---|
| 55–59 | 1,436 | 85k → 87k | 502k → 401k | 26.85M → 3.35M |
| 65–69 | 1,324 | 379k → 363k | 3.00M → 1.64M | 51.71M → 21.30M |
| 75–79 | 1,497 | 4.80M → 4.60M | 19.74M → 14.48M | 133.38M → 113.00M |
| 80–84 | 742 | 25.32M → 22.51M | 77.92M → 64.50M | 179.73M → 151.32M |
| 85–89 | 120 | 93.63M → 93.63M | 188.22M → 153.78M | 334.46M → 163.99M |
| 90–94 | 25 | 142.64M → 126.00M | 268.16M → 164.86M | 335.62M → 166.22M |
| 95–99 | 4 | 157.12M → 137.02M | 255.49M → 165.17M | 255.49M → 165.17M |

By age band (p50 / p99 / max):

| age | n | p50 | p99 | max |
|---|---|---|---|---|
| ≤18 | 1,866 | 73k → 74k | 27.21M → 14.90M | 179.89M → 153.78M |
| 19–21 | 2,968 | 171k → 159k | 79.46M → 76.05M | 171.11M → 159.50M |
| 22–24 | 1,824 | 1.32M → 1.21M | 117.00M → 113.72M | 265.36M → 166.00M |
| 25–27 | 1,463 | 2.32M → 2.03M | 171.77M → 138.11M | 335.62M → 165.17M |
| 28–30 | 1,044 | 2.38M → 2.07M | 156.54M → 127.00M | 334.46M → 166.22M |
| 31–33 | 576 | 1.07M → 1.01M | 63.73M → 52.50M | 157.12M → 128.93M |
| 34+ | 304 | 130k → 129k | 24.20M → 19.50M | 103.82M → 59.20M |

By league (p50 / p99, before → after): Premier League 978k → 656k / 193.67M → 155.66M; LaLiga 1.51M → 1.57M /
140.79M → 137.98M; Bundesliga 1.44M → 1.32M / 112.69M → 99.50M; Ligue 1 502k → 392k / 70.39M → 66.61M;
Serie A 112k → 116k / 90.38M → 83.40M; Championship 1.83M → 1.02M / 37.50M → 24.90M; the lower divisions
move by a few per cent (League One 341k → 299k; Serie B 159k → 155k; 2. Bundesliga 655k → 708k). The
unlabelled group (880 players) are players of clubs outside the 240 on loan or owned from outside.

**Largest remaining anomalies**, ratio of value to the median of players of the same OVR band, age band and
position group (anonymised). *Before*: 340×, 134×, 0.01×, 107×, 107×, 101×, 90×, 86×, 86×, 81×. *After*:
32×, 24×, 23×, 23×, 23×, 20×, 0.05×, 19×, 19×, 17×. The remaining ones are **high-ceiling teenagers**
(OVR 49–68, potential 82–91): a cohort that ignores potential cannot see why they are worth more, and they
are valued on their ceiling in the save and in the model; they were 100–340× and are now 15–32×. One
low-value outlier (an 18-year-old of OVR 60, 58k against a 1.14 M cohort) survives because the band for a
*deflated* stored value is deliberately wide (1.5 residuals).

**Not fixed, said plainly.** The estimate is least reliable for prospects and for the very top (sparse data):
Mbappé (137 M) is above Rashford (121 M) and Mainoo (120 M), but only by 13%, and the model alone would put
Mainoo level with Mbappé; I chose not to let it raise Mainoo. Tests state what the model must not do
(§10), not that the ordering is right for every pair.

## 5. Senior squad and academy classification

* **Registration decides** (`placement.rs`): first team (slot 0) is senior, a reserve or youth side is the
  academy, a player past the academy age (21) is senior. `contract.team_id` carries no information (it is
  slot 0 for 99.1% of contracts); the registration is the squad.
* **New policy**: the squad status is the one other signal, used only against the grain of the registration
  and only with evidence of level: a reserve-registered player of academy age whose status is *not* a youth
  status **and** whose ability reaches the 25th percentile of his club's first-team registrations is senior.
  Ability alone never promotes anyone. The youth status codes (10, 11, 13) are **inferred** (the codes are
  undocumented): on this save 99% of players registered with a club's own reserve or youth side carry one,
  at mean ages 19–20. They live in config beside that evidence.
* **Audit of all 240 clubs**: min 15 seniors everywhere, per-position floors met, no club empty, no
  duplicated or dropped player, no academy player over 21, no loaned player in his owner's lineup, every
  loan borrower a team of the world or named. New blocking check `squads.academy_and_loans_coherent`
  (shown to fail, and block, for each of five incoherences in a synthetic world).
* **Result: 0 classifications changed.** 7,365 senior / 2,792 youth before and after, club by club (0 of 240
  clubs differ). The rule's candidate population on this save is 10 players (reserve-registered, ≤21,
  non-youth status), and none reaches his club's floor. New signings (contract start after 1 June) are *not*
  over-represented in reserve sides (35% vs 57%), so there is no evidence of stale registrations.
* Reserve affiliations: 464 academy players of affiliated B sides stay the owner's players (no transfer);
  186 older ones are loans to the B side; no permanent transfer is invented.

## 6. Loans: ownership and displayed provenance

**Source facts.** The save has one contract per player; a loan appears only as *registration ≠ contract
club*. It carries no end date, wage share or buy option. 1,437 players are imported as loans: 557 between
clubs of the world, 694 to clubs outside the 240 (the borrower keeps its name), 186 to affiliated B sides.
The importer configures their terms (end of season, **0% wage share**).

**Changes.**

* `ActiveLoan.terms_source` = `unrecorded | agreed | sourced | estimated`. The importer marks all 1,437
  `estimated`; loans made in the game are `agreed` (asserted at the production site); a save from before the
  field reads `unrecorded`, so an old loan is **not** presented as agreed. JSON column, no SQL migration.
* The profile banner **always** shows *Contract owner → Playing for*, each club under its label (a
  `<dl>`), and labels the end date and wage share as estimates or says their source was not recorded.
* 81 players are owned by a club **outside** the world and play for one inside. I did **not** create an
  `ActiveLoan` for them: ~70 production sites use `parent_team_id` and a return would leave a team id that
  does not exist. They stay the borrower's players; `Player.provenance.outside_owner` keeps the owner's name
  (`outside_owner.club_id` is never used as a team id) and a banner says no return is scheduled. Any real
  movement (renewal, transfer, release) drops the claim.
* Tests: inside-world loans (Moscardo), outside-world ownership (all 81), affiliated teams, estimated
  terms, an agreed in-game loan, persistence of non-default values on both the INSERT and UPDATE path, an
  old save with NULL provenance and a loan with no `terms_source`, and the profile page's wiring.
* **Not done**: loan *returns* for outside owners (no mechanism exists in the game to send a player to a club
  it does not contain) and any change to wage treatment: the 0% share is unchanged and now labelled.

## 7. Histories of real players

**Cause.** Career creation runs `generate_past_world_history` for every `RosterBaseline` world, which is
what the adapter declares. It writes one season per past year, **at the player's current club**, for every
player aged 18+, with invented appearances, goals and assists. On this save, a career start with the default
12 years gave **44,917 invented entries to 7,431 imported players** (all flagged `synthetic`, shown with a
"Simulated" badge, but as a career table). The save has **no career table** (35 tables checked), so no
authentic record could be imported.

**Policy implemented.** Historical facts are supported by data or marked unavailable.

* `Player.provenance.earlier_career` = `generated | unavailable`. The importer marks all 13,045 imported
  players and pool free agents `unavailable`.
* `history_generation` writes nothing for an unavailable career: no seasons, no stats (so no historical
  award), while a season a world file recorded is still kept; for generated worlds it is unchanged.
* The profile says no verified record exists before this game, instead of an empty "first season" line or a
  table of invented seasons; seasons played in the game appear as usual.
* **Result**: 44,917 → **0** invented entries; the six probes have none. Existing saves: the column reads as
  `generated` when absent, so **nothing is reclassified**; their synthetic entries stay flagged synthetic.
* Tests: unavailable career gets no invented history (red first, mutation-checked), recorded season kept,
  current-season stats win no first-season award, a generated world still gets its history, an old save, and
  the real save end-to-end.

## 8. World revision and installation

* `WORLD_REVISION` 6 → **7**. Reason: the world's players differ (values, loan terms, provenance). It
  affects **new careers only**; a saved career keeps the players it was given. I implemented no migration of
  existing careers and **do not claim any**.
* Same source + configuration + build give the same world: two independent runs, identical SHA-256
  `df691e5ecef002630976c712928c817ff73a576a04d9bd7ab6e3c87cf1b00ab2` (24,860,415 bytes), also identical
  before and after the module refactor. Guarantee scope: one toolchain and platform (`ln`/`exp` come from the
  platform libm; values are rounded to three significant figures).
* Outdated detection: the marker's revision < 7 reports `outdated` (tested for 6 and for no marker). **The
  owner's installed world is revision 4** (marker read, file untouched), so it is already outdated.
* Nothing overwrites a world except the explicit `install_fm24_world` command (the screen offers to
  rebuild). A corrupt or missing source, a failing blocking check, or an unwritable folder leaves the
  previous world in place (existing and new tests).
* **No saves exist on this machine now** (`saves/` is absent), so I could not replay old careers. The
  upgrade path is covered by the legacy-layout tests (v061, run-1 and run-2 development layouts rewritten to
  lack v071 and loaded through the migrations) and NULL-column tests.

**To install the new world after approval** (not run by me): back up first,

```bash
cp -R "$HOME/Library/Application Support/com.apfootballmanager.app" "$HOME/apfm-app-data-backup-$(date +%Y%m%d)"
```

then open the app's world screen and use *rebuild/install* for the FM24 world (it reads
`~/apfootballmanager/data/world.db`, builds, runs the checks, replaces the world file atomically, builds the
registry beside it and writes the revision marker). A career already started keeps its old players; start a
new career to see the new values, loan labels and histories. From a terminal, to inspect a build without
installing: `cargo run --release -p fm24-adapter --bin apfm-import -- <world.db> <empty-scratch-dir>
--verify-determinism`.

## 9. Determinism and performance

| | |
|---|---|
| Importer, whole pipeline incl. valuation | 2.0 s (release); the fit is 10k × 18 normal equations, not separately profiled |
| Determinism | two runs, same SHA-256 (above) |
| New-career creation, 12 years of history | 114.9 s before → 111.7 s after (clean runs, release). History depth 0: 111.5 s |
| Registry build | not separately timed |

Removing the invented histories **saves nothing measurable**: history was never the bottleneck of new-career
creation, which costs ~112 s whatever the depth. That 112 s is pre-existing and was not investigated (a
profile run at 142.9 s was taken while another build was running and is not used).

## 10. Verification

| Gate | Result |
|---|---|
| `cargo fmt --check` | pass |
| Clippy `-D warnings`, default and `--features mcp`, workspace, all targets | pass |
| `cargo test --workspace` (`a1a2c3a7`) | **5,389 passed, 0 failed**, 135 ignored (168 targets) |
| `cargo test --workspace --features mcp` (`a1a2c3a7`) | **5,476 passed, 0 failed**, 135 ignored |
| Affected crates after the test hardening (adapter, domain, db, ofm_core) | **4,256 passed, 0 failed** |
| Real-source tests (`APFM_WORLD_DB` given to the test process only), 10 | 10 passed |
| Vitest (final, full) | **3,284 passed**, 0 failed, 4 skipped (373 files; baseline 3,272) |
| `tsc`, `npm run build`, Biome lint (3 pre-existing infos) and format, Knip, i18n gate | pass |

The real-source tests are `#[ignore]` (run with `--include-ignored`); without the source they show as
ignored rather than passing with no assertion. A forced run of one *ignored* measurement study in
`ofm_core` (`ai_to_ai_transfers_are_a_small_part_of_how_wage_bills_move`) fails with "no AI-to-AI
transfers"; it **fails identically at the baseline commit** (checked in a temporary worktree) and is not a
gate.

**Test discipline, honestly.** The history and outside-owner tests were seen failing first. The valuation,
placement and UI tests were written *alongside* the code; the test reviewer then found surviving mutations,
and each is now killed by a named test (every mutation was re-applied and the file restored): the status
rule's switch, inclusive floor, unset status, loan branch and perimeter; `from_row`, `contract_end_for`, the
ceiling and the trust asymmetry; the pipeline wiring (provenance, estimated loans, the value actually
written, the check failing); `Agreed` at the in-game loan site; the DB UPDATE path; the profile wiring.

**Reviewers** (read-only subagents on `ea177fb9..HEAD`): architecture, dedup, i18n, accessibility, test.
None found a P0. Fixed: a stale outside-owner claim after a move (real defect, introduced by this work);
`pipeline.rs` back under the 1,500-line ceiling (1,624 → 1,471 production lines); the registry's duplicated
fallback; the history guard order; a borrower check; the audit tool refusing an output path equal to the
source; the estimate strings aligned with their siblings; accessibility semantics and tokens; and all the
test gaps above.

## 11. Remaining defects and open items

* **Prospects and the top end** are the weakest part of the model (§4). Near-ties at the top are model
  limits, not a verified ranking.
* **FM scale vs the game's native value formula** (about ×10): pre-existing; after a season, projected
  external players are valued on the native scale.
* **Squad-status semantics are inferred.** The rule changes no player here; its value is for other saves.
* **2,118 of ~9,960 perimeter contracts start after the configured 1 June start** (1,161 on 13 July): the
  save is dated mid-July and the adapter's start date is a configured choice. Unchanged.
* Invented **team, manager and league-position histories and rivalries** for real clubs are still generated
  at career creation (I changed player histories only, as asked). Not measured, not fixed.
* A player later promoted from the external registry carries the default provenance (his profile shows the
  ordinary "no career" line; nothing is invented about him). Marking him needs a newgen flag on the registry
  type.
* Outside-owner players cannot be bought back or returned; see §6.
* Locale quality for native review: cs/zh-CN/ru/de "contract owner" are literal; id "kepulangan"; the
  Turkish `%{{percent}}'i` suffix flaw is pre-existing and copied. Contrast of the new notes in both themes
  and narrow-width wrapping were **not measured**; the "Estimated" signal is wording, not a badge.
* Duplications recorded for epic #589: quantile helpers (`report.rs`, `free_agents.rs`, `senior_floors`,
  examples), a banner shell for three profile banners. Size: `pipeline::run` (350), `build_registry`, and
  `run_checks` (~840 lines) were already over the 100-line ceiling and grew; `apply_valuation` has 7
  parameters (the cap); `examples/data2_audit.rs::main` is a 435-line tool function.
* The `integrity.world_audit` info check reports 75 clubs above 45 players (loanees and academies); player
  counts per club are identical before and after, so this is not new.
* The actual macOS application was not operated: no display. The four flows (profile → loan banner, an
  outside-owned player, a career with no history, install and new career) need a person.
