# APFM I3: validation

Evidence and its limits for [`APFM_I3_INTEGRATION.md`](APFM_I3_INTEGRATION.md). Branch
`integration/i3-unified`. Host: macOS, Apple Silicon. Labels: **PASSED**, **FAILED**, **NOT RUN**. A local
pass is not a claim about CI. Nothing was pushed.

## 1. Verdict

**CONDITIONAL GO** for the local `apfm/main` update and for playing the integrated game.

For: every automated gate passes on the integrated tree; the three ten-season generated careers and three
ten-season imported-registry careers finish with clean invariants, no mutating refusal, no ledger mismatch, no
duplicate player and an identical reload twin; the release `.app` builds from this tree and starts, with the
frontend and the Rust IPC talking; the one cross-branch defect is fixed test-first.

Against a plain GO:

1. **No person, and no driver, used the desktop UI.** Startup is verified; window presence and every interactive
   step are NOT RUN (§8).
2. **"No squad exceeds 30 seniors" does not hold in the generated-world study.** Four AI-club season rows reach
   31. It is identical on the I2 base and on I2.1, so it pre-dates this integration and is not fixed here (§5.3, §9).
3. The Squad tab scrolls sideways at the 960×640 minimum window; pre-existing (§3.3).

Why not NO-GO: no regression against I2 or I2.1 was found anywhere, and no critical or high-severity defect
remains open.

## 2. Gates on the integrated tree

Final Rust tree `ed13f93e`; the later commits only add a frontend test and documentation.

| Gate | Command | Result |
| --- | --- | --- |
| Rust format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | **PASSED** (it failed once on my own unformatted line, fixed in `ed13f93e`, then re-run on the final tree) |
| Clippy | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings` | **PASSED** |
| Clippy, MCP | same with `--features mcp` | **PASSED** |
| Workspace tests | `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace --no-fail-fast` | **PASSED**: 152 result lines, **5,055 passed, 0 failed, 121 ignored** |
| Library tests, MCP | `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --features mcp` | **PASSED**: 673 passed, 0 failed, 10 ignored |
| Frontend suite | `npm test` | **PASSED**: 345 files passed, 1 skipped; **3,053 passed**, 4 skipped |
| Lint | `npm run lint` | **PASSED** (3 informational suggestions) |
| Format | `npm run format:check` | **PASSED** |
| Type check + build | `npm run build` | **PASSED** |
| Dead code | `npm run knip` | **PASSED** (two configuration hints) |
| i18n reporter | `npm run audit:i18n` | exits 0; "All supported locales match the English key set"; the heuristic candidates it lists are pre-existing and outside this change |
| i18n gate | `npm test -- src/i18n` (inside the full suite) | **PASSED** |
| Design-system rules | `src/designSystem.test.ts` (inside the full suite) | **PASSED** |
| Global Market interaction | `node scripts/global-market-gui-check.mjs http://localhost:1420` | **PASSED**: "25 rows, signing, paging and focus behave" |
| Page layout | `node scripts/page-layout-check.mjs http://localhost:1420` | **14 of 16 pass**; two failures, §3.3 |

Counts against the sources: Rust 5,053 (I2.1) + 2 new domain tests = 5,055; frontend 3,039 (I2.2) + 1 panel test + 13
contract tests = 3,053. `--locked` was used for every Rust gate. `npm ci` was run first and `npm run <script>` used
throughout (never `npx`); Biome reported 2.5.5.

### Red then green (the I3 fix)

| Step | Evidence |
| --- | --- |
| Rewritten test fails for the predicted reason | `a_free_agent_lapses_when_the_last_place_is_promised_before_he_answers`: `left: Some(ClubCannotAfford)`, `right: Some(NoSeniorPlace)` (variant added, behaviour not yet) |
| Passes with `lapse_reason` | same test green; the 22 tests in `free_agent_talks_tests` pass, including the unchanged money-lapse test |
| Locale gate | `localeCoverage` red with only `en.json` changed (the key missing in 11 locales), green after the real translations |
| Contract test can fail | `squadFull` deleted from `fr.json`: `gives every backend refusal its own sentence in fr` failed on that key; green again on restore |

## 3. Browser harness

The dev page harness mounts the real dashboard with a deterministic 30-player registry; headless Chrome drives it
through the DevTools protocol. This is the React app in Chrome, **not** the Tauri webview.

### 3.1 Interaction

One run: 25 rows on page 1 with a caption; selecting a player moves focus to the profile region and sets
`aria-current`; checking terms reads "Agreed" with Sign enabled; the dialog holds focus; confirming closes it and
focuses the success status; paging reads "Page 2 of 2" then "Page 1 of 2" with focus moved off the disabled control;
no text under 12 px; no sideways page scroll.

### 3.2 Layout, Global Market

Passes at 1440×900, 1280×800, 1024×768, 900×650 and 640×800 (en), and at 1024×768 and 960×640 extra-large (de).

### 3.3 The two layout failures

```text
FAIL squad 960×640 normal en: the page scrolls sideways (1194 > 960)
FAIL squad 960×640 xlarge de: the page scrolls sideways (1522 > 960)
```

The Squad tab is outside the Global Market and this integration. To check the claim of I2.2 that it pre-existed
rather than trust it, the same script was run against an export of the I2 tree (`73db8d43`, before I2.2): the
Squad tab scrolled sideways there too (1244 > 960 at 960×640 normal en), wider than on I3. (That run also showed
other failures, "unanswered commands", because the I2 harness fixture lacks answers the newer script asks for; they
are an artefact of mixing versions, not findings.) The window's configured minimum is 960×640, so a user can reach
this size.

## 4. Cross-branch contract

I2.2's screen was written against a mock of the I2.1 backend; the only direct check between them is the contract
test (`globalMarketBackendContract.test.ts`, 13 refusal keys × 12 locales, §2). The scenarios the brief named, with
what actually proves each:

| Scenario | Evidence | Kind |
| --- | --- | --- |
| Successful signing | `a_recruit_survives_a_save_and_reload`; harness run in §3.1 (signing then status) | backend test; browser harness against a fixture |
| Rejected signing | `GlobalMarketTab` tests "shows a refusal with its reason and keeps the signing button closed" and "explains why the signing control is closed until the terms are agreed" | component, mocked backend |
| A promised place blocks a new signing | `a_global_market_free_agent_cannot_take_the_place_an_agreed_purchase_was_promised`, `a_purchase_is_refused_when_the_last_place_is_promised`, `a_full_list_with_a_pending_purchase_never_exceeds_the_limit`; the UI shows the `squadFull` sentence with its numbers (component test "resolves a blocker that carries parameters, so a full squad reads as a sentence", and the contract test) | backend tests through production entry points; component; contract |
| A pending purchase registers into its own place | `a_pending_purchase_does_not_block_its_own_registration`, `a_pending_purchase_registers_once_a_place_frees` | backend |
| A free agent signs after a reservation is released | `a_global_market_free_agent_signs_once_the_promised_place_frees`, `a_free_agent_talk_is_accepted_once_a_place_frees`, `a_withdrawn_pending_purchase_releases_its_place` | backend |
| Insufficient transfer budget / cash | the career studies' `transferBudgetTooLow` (182–331) and `insufficientFunds` (101–120) refusals, all non-mutating; contract test shows each as its own sentence | career studies; contract |
| Signing-bonus affordability | `a_free_agent_bonus_is_judged_against_the_fees_of_agreed_deals`, `a_fee_and_bonus_together_cannot_overdraw...` | backend |
| Wage-policy refusal | `boardWagePolicy` refusals (1,130 / 1,356 / 1,279 across the three imported careers), none mutating; contract test | career studies; contract |
| Club counteroffer / player counteroffer / player refusal | `GlobalMarketTab` test "does not sign a counter: accepting their terms submits the counter first" and the `globalMarketModel` `signHintKey`/`canSign` tests; `the_legacy_preview_agrees_with_the_attempt_when_the_player_refuses_the_move`; the careers' `playerRefusesMove` and `negotiationNotAgreed` | component; backend; career studies |
| Invalid negotiation state | `negotiationNotAgreed` refusals, covered by the contract test | career studies; contract |
| Backend command failure | component test "replaces an unknown backend key with a readable market error"; transport failure gives the generic sentence | component |
| Unknown translation key | `marketErrorMessage` tests; contract test | model; contract |
| Save and reload with pending transfers | `a_promised_place_survives_a_save_and_reload`; the reload twin in every career study | backend; career studies |
| A failed signing must not mutate state | the I2.1 refusal tests compare the serialised game before and after; "refusals that mutated: 0" in all six careers | backend; career studies |
| Success not reported before the backend confirms | component tests "does not announce success while the backend is still answering" and "shows an agreed profile and only moves the career once the manager confirms"; the confirmation dialog is the only path and stays busy during the call | component |

**Limit.** The rows marked "component" run against a mocked service layer, and the Rust rows against no UI. No test
joins them through live Tauri IPC with a rendered screen, and no person did. That joined path is what manual
acceptance (§8) is for.

## 5. Long-career validation

Release builds. Deterministic seeds.

### 5.1 Generated world, three seeds, ten seasons (RUN)

```text
P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 \
  cargo test --release --locked --manifest-path src-tauri/Cargo.toml \
  -p season-harness --test p45_long_career p45_long_career \
  -- --ignored --nocapture --test-threads=1
```

| Seed | Seasons | Players at start | Invariants | Clubs in debt (every season) | Seniors max (any season) |
| --- | --- | --- | --- | --- | --- |
| 1 | 10 | 3,742 | clean | 0 | 31 (season 5) |
| 2 | 10 | 3,744 | clean | 0 | 31 (seasons 4 and 5) |
| 3 | 10 | 3,698 | clean | 0 | 31 (season 6) |

144.5 s for the three seeds with invariants on (about 48 s per seed; I2.1 recorded 145.3 s). Squad minimum was 19–21.

### 5.2 Imported-registry careers, three seeds, ten seasons (RUN)

```text
I1_WORLD=gate I1_SEED=<1|2|3> I1_SEASONS=10 \
I1_WORLD_DB=$HOME/apfootballmanager/data/world.db \
  cargo test --release --locked --manifest-path src-tauri/Cargo.toml --lib \
  commands::registry::combined_career::i1_combined_career -- --ignored --nocapture
```

What this is: a generated career played through the production day and rollover, in which the manager shops in the
**global registry built from the player's own imported `world.db`** (SHA-256 prefix `36b6908c`, the same as I2;
81,762 stored, 69,301 eligible, 9,843 active-world), then saves, reloads a twin and compares. It is the study I2 and
I2.1 call the imported-world combined career. It is **not** a ten-season career in the 240-club imported FM24 league
world (§7).

| Seed | Free agents / contracted signed | Fees | Mutating refusals | Ledger mismatches | Duplicates | Invariants | Reload twin (at reload / 60 days on) | Market | Clubs in debt |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 60 / 8 | 42.47 M | 0 | 0 | 0 | clean | identical / identical, same market | 66.6 k | 0 each season |
| 2 | 34 / 8 | 62.11 M | 0 | 0 | 0 | clean | identical / identical, same market | 66.6 k | 0 each season |
| 3 | 34 / 8 | 27.55 M | 0 | 0 | 0 | clean | identical / identical, same market | 66.6 k | 0 each season |

Free agents in the market stayed at 17.7–18.9 k. The user's club held 10–23 seniors, so this study never presses
against the 30 limit and says nothing about it. Newgens per 100 of the best grew from 8–16 to 77–81 over ten seasons.

### 5.3 Comparison and the 31-senior rows

| Run | Tree | Table lines that differ from I3 |
| --- | --- | --- |
| Generated world | I2.1 tip `1f13ddc7` | none except wall-clock lines |
| Generated world | I2 base `73db8d43` | none except wall-clock lines |
| Imported registry, all three seeds | I2.1 recorded results | signings, fees, refusal counts equal to the digit |

The generated-world output of I2, I2.1 and I3 is identical, including the four rows with 31 seniors. So:

* I3 changed no simulated outcome, as intended: the new reason is a label on a lapsed talk.
* The I2.1 validation statement that every season's maximum "was at most 30" is **not true** for this study. The
  rows were there at I2 as well.
* I2.1 predicted that AI free-agent behaviour would change; on these seeds its reservation gate changed no row.
* The study's squad count is every player with the club and the senior role at the end-of-season sample. The harness
  invariants contain no check of the 30 limit, which is why "invariants: clean" coexists with 31.
* I2.1 §9 items 1 and 3 name loan-ins and academy promotions as paths that take a place without a capacity check. That
  matches, but **I did not trace which path produced these four rows.**

Closing it changes simulation outcomes and belongs in its own phase with its own baseline; it was not attempted
inside an integration.

## 6. Reviewer agents

**NOT RUN** for this integration (`ofm-architecture-reviewer`, `ofm-dedup-reviewer`, `ofm-test-reviewer`,
`i18n-auditor`, `ui-accessibility-reviewer`). I2, I2.1 and I2.2 ran their own self-checks; I3's production change is
a ten-line classification plus a locale key. `/preflight` was not run as one invocation; its components were run
individually (§2).

## 7. Not run

* `cargo deny`, `cargo machete`, CI, a thirty-season career.
* A ten-season career in the 240-club imported FM24 league world. The installed
  `databases/apfm-fm24-2026.json` is the kind of file the project's notes call a stale-world trap, and regenerating
  a fresh world was not part of this run. I2 ran 5-season imported-league careers and the 22 `fm24_*` suites; those
  were not repeated.
* The `fm24_*` ignored suites (they need `APFM_WORLD_JSON`).
* Translation review by a native speaker for the new `no_senior_place` sentences; they follow each locale's existing
  wording for the senior list but were not reviewed by a speaker. The I2.2 pt-BR, cs and ru wording changes are
  likewise unreviewed by a speaker.

## 8. Desktop application

### 8.1 Build and startup (RUN)

```text
npm run tauri -- build --bundles app
```

Built `AP Football Manager.app` (30.6 MiB) in 4 min 33 s from the I3 tree. The binary inside the bundle was
launched with `HOME` pointed at an empty scratch directory, so that it could not read, migrate or overwrite the real
saves under `~/Library/Application Support/com.apfootballmanager.app`. (Those files kept their old modification
times.)

| Check | Observed |
| --- | --- |
| Process | alive 63 s until stopped by me; RSS about 107 MB |
| stderr | empty |
| App log | 0 lines matching error, panic, fatal or warn |
| Frontend | the webview loaded the bundled assets and logged `[startup] AP Football Manager v0.3.0-nightly · 90b850c` (the HEAD commit at build time) |
| IPC | Rust received `get_settings` and `get_active_game` from the frontend: the real Tauri bridge is wired |
| Fresh data dir | created `device_id`, `saves/`, `Logs/` under the scratch home |

**Startup: PASSED.**

### 8.2 GUI interaction: NOT RUN

Career creation or loading, squad, tactics, calendar, match simulation, Global Market, negotiation, signing, and
save/reload through the window were **not exercised**. A query for the window through System Events blocked on a
macOS Automation permission prompt that I did not and should not approve on the user's behalf; it was stopped. No
screenshot or window listing was obtained, so even "a window appeared" is not claimed. The browser harness in §3 is
evidence about the React app, not about this shell.

### 8.3 Manual acceptance (required)

Run the built app (see the completion report for the exact command) and, in order:

1. Start a new career (or load a save); the dashboard opens with no error dialog.
2. Squad: open the squad, pick a formation and a role for a slot; the formation-fit figure updates.
3. Tactics and Calendar: save a tactic; open the calendar; play a day and a match; the result and standings update.
4. Transfers → Global Market: tab to a player, press Enter; focus lands on the profile; check the terms; take a
   counter-offer with "Use their terms"; Sign is only enabled once the terms are acceptable.
5. Sign; confirm; the success message appears only after the call and receives focus; the player is on the squad
   and cash and budget moved by the fee and bonus.
6. Make the squad 29 seniors with one purchase agreed and waiting to register; try to sign a second player (Global
   Market or free agent): the squad-place sentence appears with its numbers, and nothing changes.
7. Free-agent talk lapsing for want of a place: open an offer to a free agent, fill the last place before he answers,
   advance two days: the lapse says there was no senior place, not that the club cannot afford it.
8. Break the registry or deny the command: a readable alert with Try again, no raw key.
9. Save, quit, relaunch, load: the squad, cash and any pending transfer are as left.
10. Switch the language to pt-BR, cs and ru and read the Global Market and the lapse sentence.
11. Resize to the 960×640 minimum and check the Squad tab (§3.3 predicts a sideways scroll).

## 9. Remaining issues

| Severity | Issue | Status |
| --- | --- | --- |
| MEDIUM | AI clubs reach 31 seniors in four season rows of the generated-world study (§5.3). Pre-dates I2.1; loan-ins and academy promotions take a place without the capacity check; a pending loan-in reserves no place (I2.1 §9). Not traced to a specific path here. | Open, pre-existing, not changed |
| MEDIUM | GUI acceptance NOT RUN (§8.2). The shell starts and IPC is wired; no screen was operated. | Open: manual acceptance |
| MEDIUM | Squad tab scrolls sideways at the 960×640 minimum window (§3.3). | Open, pre-existing |
| LOW | The I2.1 validation report says the generated-world maximum was 30; it was 31. | Recorded here; the source branch was not edited |
| LOW | A lapse caused by anything other than the squad limit (wage policy, cash, a missing club) is still reported as `club_cannot_afford`. | Intended, unchanged |
| LOW | New and I2.2-changed locale sentences are not reviewed by native speakers. | Open |
| LOW | A save carrying the new reason will not load in a build that predates it. | Accepted forward incompatibility |
| LOW | `cargo deny`, `cargo machete`, reviewer agents, CI, a 30-season career, and an imported-FM24-league ten-season career were not run. | Not run |
| LOW | Duplicated negotiation validation inside D7B3 (I2.1 §9 item 4); no production command cancels a pending-registration purchase (item 5). | Carried over, unchanged |

No CRITICAL or HIGH issue is open.

## 10. Final verdict

**CONDITIONAL GO.** The merged tree builds, passes every automated gate, plays ten seasons across six careers with
clean invariants, and starts as a desktop application. It is not "fully validated": the desktop UI has not been
operated, and the 30-senior ceiling does not hold for AI clubs in the generated-world study, as it did not at I2.
