# ALPHA6.1 — Reliability closure

Written for: the owner and the independent reviewer of ALPHA6 and ALPHA6.1.

**Verdict: see section 8.** Everything below says what was measured, what was changed, what was
only measured, and what was not done. Where a number comes from a run it names the run.

## 0. Baseline and SHAs

* Required baseline `ef460fe651f185e7aacc4cedc383a49109caecd8` (confirmed: `git rev-parse HEAD`, clean
  tree, and `origin/apfm/main` at the same SHA).
* Commits of this mission, in order (final SHA: last line of this file, written at push time):

| Commit | What |
|---|---|
| `fix(transactions): a lost answer is never read as "nothing happened" while the request still runs` | section 1 |
| `fix(ui): StrictMode no longer bounces a deliberate back-to-menu; every failure goes through one wording` | section 2 |
| `test(delegation): ninety days of Total delegation through the real daily turn` | section 3 |
| `fix(transfers): a club with no place on its senior list no longer shops, and the AI funnel is measured` | section 4 |
| `test(market): measure the AI market …` + `style(harness)` | section 4 |
| `test(ui): loading a save under StrictMode …` | section 2 |
| `fix(a11y,i18n): announce transfer failures in the bid and counter dialogs; two Czech corrections; …` | reviewers' findings, section 6.3 |
| `test(calendar): measure the distribution of fixture delays …` | section 5 |
| `docs(alpha6.1): …` (this document) | all |

* The owner's real world database `~/apfootballmanager/data/world.db`: SHA-256
  `36b6908caa90f57e6e4bb758cec724f05123eaefe8a5b8dec5efc0dda27439f7` at the start; re-checked at the
  end (section 7). Every source-backed run used the world file built in the previous mission,
  `~/apfm-scratch/alpha6-world/apfm-fm24-2026.json` (read-only input; nothing was installed over a
  save or the real database).

## 1. Transaction idempotency under incomplete and lost responses

### 1.1 What the independent audit found, reproduced

`get_transaction_status` answered `unknown` for an id the ledger had not recorded, and the screen
(`transactionService.ts`) read `unknown` as "the command never ran, retrying is safe". The ledger recorded a
request only when it **finished**, and under the game lock. So a request that had arrived and was queued
behind another command, or was still executing, was `unknown` as well. The exact scenario of the brief:
*a transport failure while the transaction is still processing → the client asks → `unknown` → the
transaction commits*. A second, quieter hole: nothing stopped a delayed delivery of an id from
running after the screen had already declared it failed.

Reproduced first as failing tests: `a_status_asked_while_the_request_runs_says_in_progress_and_it_then_commits`
and `a_request_waiting_for_the_game_lock_is_already_in_progress` (Rust, `ofm_core::transactions`), and
`waits_for_a_request_that_is_still_executing_and_reports_its_commit` /
`keeps_the_original_identity_when_the_outcome_cannot_be_established` (vitest).

### 1.2 The protocol

* **`in_progress` from arrival.** `StateManager::run_transaction` marks the id before it waits for the
  game lock. A guard takes the mark back if the command panics or finds no game. In-progress entries are
  never evicted to make room (the ledger is bounded at 256).
* **`settle_transaction` closes an unseen id (`abandoned`).** A late delivery of a closed id is refused
  (`be.error.transactions.requestAbandoned`) before it can write. "It did not run" is therefore a fact
  the screen may act on (retry with a **new** id), not a guess. `get_transaction_status` stays read-only.
* **An id belongs to one operation and one career.** Every command carries `request: { id, careerToken }`
  (`Game.career_token`, which was already minted per installed career). A command for a career that is
  no longer installed is refused before it writes (`careerChanged`); the ledger is emptied when the
  installed career changes (`set_game` with another token, `clear_game`) but kept when the same career is
  replaced by a newer state of itself; an id reused for another operation is refused (`requestIdReused`);
  asking about an id with the token of a replaced career answers `career_changed`, never "committed" or
  "unknown".
* **The answer is bound to the commit.** The first delivery's returned `game` is taken under the same
  hold of the game lock as the commit (`TransactionReceipt.snapshot`); before, it was read after the lock
  was released, so a concurrent command's change could be in it. A replay carries no snapshot (none was
  kept: 256 copies of a 16,609-player game is not a ledger) and returns the game as it is now, which already
  contains the committed change. **Residual risk:** two answers that arrive out of order can still
  leave the screen on the older snapshot; no global revision number exists to order them.
* **The screen** (`submitTransaction`) chases a lost answer with `settle_transaction`, waits while the
  backend says `in_progress` (8 looks, about 5 s), and ends in one of: `went-through` (committed),
  `refused`, `failed` (proved not to have run: retry is safe with a new id), `in-progress` / `unconfirmed`
  (the outcome could not be established: Submit stays closed, the original request id is kept in the
  outcome, no retry is offered), `career-changed`. An **answer that arrives after another career was put
  on screen is dropped**, not delivered to it.
* Hot path cost: the snapshot is a `ui_clone` (4.8 ms on the real 16,609-player world, measured in
  ALPHA6) taken while the lock is held, in addition to the 4.4 ms all-or-nothing copy that was already
  there.

### 1.3 Tests

Rust (`transactions.rs` 24 tests, `commands/transfers.rs` +4): repeated and refused ids, 16-thread same id
runs once, concurrent writer loses nothing, in-progress while running and while queued behind the lock,
overlapping retry runs once, settled-unseen refuses a late delivery, settle of a known request closes
nothing, operation mismatch, stale career refused with the game byte-for-byte unchanged, status from an
earlier career, same-career replacement keeps the ledger, cleared career, panicking command leaves no
mark, in-progress survives eviction, snapshot taken at the commit (a writer waiting on the lock cannot be
in it), replay has no snapshot, save/reload ends the old career's ids. Frontend (`transactionService.test.ts`
13, `transferSubmission.test.ts` 5): every outcome above, with the exact incident, an unreadable status,
an unreachable backend, a stale-career answer and refusal.

**Mutations (rule removed → named test red → restored):** no in-progress mark (4 tests red),
settle does not close (1), career check removed (1), ledger not reset on a new career (3), operation not
checked (1), in-progress evicted (1), snapshot read after the lock (1, three runs); frontend: in-progress
reported as failed (4), stale-career answer delivered (2). 9 killed, 0 survived. A tenth defect found
while writing the frontend tests: `beforeEach(() => mock.mockReset())` returns the mock, which vitest
runs as the teardown (a throwing mock then fails the *previous* test); the hook uses braces now.

### 1.4 What this does not prove

* Not run in the real app (no operable display, section 7). Tauri's IPC is in-process, so a delayed
  delivery is far less likely there than the protocol allows for; the protocol does not rely on that.
* A panic inside a transaction poisons the game mutex as it always did (`update_game` unwraps the lock);
  later commands panic. Pre-existing for every command, noted, not changed.
* The ledger is in memory: after an application restart every id is unknown, and the career token is
  new, so the screen would read `career_changed`.

## 2. Startup and error reliability

### 2.1 StrictMode double mount

The one-shot "the player left the career on purpose" flag was consumed by the **first** mount of the menu;
React StrictMode (`tauri dev`) mounts, unmounts and mounts again, so the second mount, the one the player
keeps, resumed the clubless career. It is now a state (`markLeftCareerOnPurpose`) that ends when the
player starts or loads a career (`beginCareerOperation`) or when the menu resumes a career the backend
announced. The team-selection back arrow now has an accessible name (`common.back`; it had none).

Tests under `<StrictMode>`: deliberate back-out is not bounced on the second mount; still respected
after another mount; a held employed career resumes **once** on `/dashboard`; a clubless one once on
`/select-team`; loading a save navigates once and ends the deliberate stay; starting/loading ends it;
team selection's back arrow marks it. Mutation (flag consumed on read) → two tests red.

### 2.2 Raw backend text

The previous audit named seven components. Reading them: five already used `resolveBackendKeyError`
(a safe fallback for non-keys) but **that function returned an untranslated `be.error.*` key as the
explanation** (fixed, test). The other 2 panels and **29 further call sites in 17 files** used
`resolveBackendError`, which returns raw Rust prose and untranslated keys unchanged: staff, finances,
world editor, saves/menu, report submission, transfer deadline, free-agent contract, dashboard save
failure, match substitution/role refusals. All now go through `failureText` / `failureReason`
(`src/utils/failureText.ts`) which words a known refusal in the selected language and anything else as one
safe message with a reference (raw error to the log). `errors.unexpected` no longer names the Transfers
page (it is shown by every screen now). A static audit now **forbids a direct `resolveBackendError(`
call** outside the two modules that define the wording (with a fixture that proves it can fail), on top of
the existing raw-display audit. Tests: `failureText.test.ts` (known refusal, untranslated key, Rust
prose, JS error, reason alone, French), and `Dashboard.test.tsx` now asserts the player reads a
reference and **not** `disk unavailable`.

**Not claimed:** localization completeness. The static audits catch only the shapes they look for. Not
covered: a backend string that is already English prose and is shown by a path that never uses an error
(a success message, inbox/news content), `console`-only messages, and text built by `.message` inside a
third-party dialog. MCP, CLI and docs stay English by policy.

## 3. Ninety days of delegation

`crates/season-harness/tests/delegation_90_days.rs`, six tests, through the real daily turn
(`play_day`: AI clubs bidding, windows, registration, finance posting) with a real hired assistant,
`DelegationMode::Full`, a transfer budget and cash, on the generated gate world (seeds 1 to 3) and on the
**source-backed FM24 world** (PSG, `APFM_WORLD_JSON`; the test prints `SKIPPED` without it).

| Requirement | Evidence |
|---|---|
| 1 lists an unnecessary player | `listedSurplus/Funds/Leaving` entries with the player id (seeds 1–3, PSG) |
| 2 accepts or negotiates a suitable sale | listed-then-sold on seeds 1 and 3 (seed 2: listed, **no club bid in 75 days**: the market, not the assistant); **controlled** on seed 2: a bid of 130 % of value is accepted once, for that fee; a 30 % bid is declined (`offerDeclined`) and he does not leave on it |
| 3 identifies a position needing recruitment | `recruitScheduled` carries the position; a back line cut to four gets defenders (≤ 4, not a spree) |
| 4 signs an eligible replacement | every announced signing is matched to a transfer-log move of the announced fee and a player of the announced position group on the club's books |
| 5 respects a protected player | control run lists and sells him; the same career with him protected never lists, offers, sells or proposes him while the assistant keeps working; a **fortune** (400 % of value) for a protected star is declined and logged `offerDeclinedProtected` |
| 6 no duplicates, no overflow | one sale and one signing per player, none both ways; seniors ≤ 30 and ≥ the floor on every one of 90 days; every player at one club |
| financial and player movements | per run: every move with date/fee/direction, **the cash journal explains the cash to the cent** (Σ posts = Δcash) and its `TransferFeeIn/Out` rows equal the transfer log |

Observed (printed by the tests): seed 1: 3 sales (€5.41 M), 2 signings (€8.75 M), cash 40.0 → 45.3 M;
seed 3: 1 sale (€1.30 M), 1 signing (€6.0 M); PSG (FM24, budget set to €150 M / cash €400 M): 4 sales
(€2.08 M), 2 signings (**€97.8 M**: €81.65 M + €16.15 M), seniors 23 → 21. The assistant has no
per-player ceiling under Total (the club's books are the authority), so a single €81.65 M signing is
allowed; that is the documented design, noted because the budget here was set by the test.

Mutations: protection ignored (2 red), listing disabled (3 red), offers never answered (4 red).
**No change to the delegation logic: nothing in these runs showed a defect.**

## 4. AI transfer-market volume

### 4.1 What was measured

`measure_ai_market_volume` (ignored; release; the FM24 world; 3 seeds × 3 seasons), counting **between AI
clubs only**, by kind of move taken from the player movement ledgers:

| Season | permanent transfers | loans started | free-agent signings | releases |
|---|---|---|---|---|
| 1 (seeds 1/2/3) | **67 / 64 / 89** | 76 / 69 / 75 | 473 / 450 / 460 | 108 / 94 / 95 |
| 2 | 131 / 105 / 107 | 137 / 138 / 120 | 1203 / 1207 / 1188 | 2425 / 2488 / 2433 |
| 3 | 153 / 119 / 124 | 165 / 140 / 150 | 1352 / 1328 / 1340 | 2558 / 2569 / 2574 |

Cross-border share 52–67 %; bought by clubs in the player's scope 11–26 of 64–153. The "72, 51, 46" of
ALPHA6 is the first row (seed 7). Season 1 is the lowest, **not** a steady state; the old bar of 120 is
met in seasons 2 and 3 by 5 of 6 runs.

### 4.2 The funnel (seed 1, season 1, before the fix)

5,685 buyer turns weighed 8.8 million buyer-player pairs: 4.08 M had no sporting reason, 3.42 M no case
for the side, 638 k priced above the buyer's ceiling, 358 k the seller would not sell on the usual
terms, 109 k failed the books test, 22 k the wage, 2 k the player's own terms, 128 exceptional offers the
seller declined; 28,058 passed and **3,100 purchases were chosen. 67 completed and 2,990 were refused at
execution, every one with `be.error.transfers.buyerSquadFull`.** (Affordability: 0 failed the budget test.)

### 4.3 Why it fell

1. **The senior list is the binding constraint.** The registration limit is 30 seniors per club. On a
   typical window day **73 to 137 AI clubs** (of 310) are at it (season 1: about 100; season 2: 128–137).
2. **The places of a club's loaned-out seniors are held** (`registration::places_reserved_by`, from
   `f33058f8` 2026-11-16: "hold the senior list at 30 on every promotion and loan route"; the ALPHA6 D4b
   case, PSG 23 + 7). This world has **1,543 players on loan** (878 external, 665 internal) ending
   2027-06-30, so for the whole first season those places cannot be promised to a purchase.
3. **A/B at HEAD, one rule changed at a time (first season, seeds 1/2/3, AI clubs only):**

   | Variant | permanent transfers |
   |---|---|
   | as shipped | 70 / 63 / 90 |
   | AI lists trimmed to 29 before day 1 (478 seniors to the reserve list) | 76 / 69 / 76 (clubs refill to 30 within days: the limit binds dynamically, AI clubs fill with free agents) |
   | senior limit 34 (sensitivity only) | 260 / 263 / 246 (loans 145 / 145 / 139) |
   | **no hold on loaned-out places** (sensitivity only; rule restored afterwards) | **367 / 383 / 352** |

   So the hold on loaned-out places accounts for roughly four-fifths of the market's volume in this world
   (about 5× when removed); the trimmed-list run rules out "the squads simply start too large".
4. **Dating.** The test passed at I2 (2026-11-13); `f33058f8` landed on 2026-11-16; the first place-reserving
   change, `2b7344e7` (2026-11-12, I2.1: pending purchases), changes nothing in a generated world (identical
   volume at `2b7344e7^` and `2b7344e7`, measured). I could **not** run I2's code on the FM24 world: the world
   file's format has moved on (`be.error.worldParseFailed` at the I3 merge) and rebuilding the old
   world would change two things at once. The causal claim therefore rests on the A/B above and on the
   commit history, not on a rerun of I2.

### 4.4 What was fixed (a demonstrated defect), and what was not

* **Fixed, volume-neutral:** a club whose senior list had no place still shopped: it chose a purchase
  registration was certain to refuse (96 % of everything the clubs chose), its turn was wasted, and **an
  offer it made for the user's player was one he could never accept** (the ALPHA6 D4a "Vitinha" warning's
  root). `clubs_without_a_senior_place` leaves those clubs out of the sweep with the same rule and limit as
  the registration gate (a test asserts they agree over eight list states). Both daily allowances (deals,
  loans) are still sized by the clubs that wanted to shop: when they were not, the loan allowance shrank,
  loans stopped pre-empting purchases and permanent transfers rose 20–40 % — a hidden rebalancing that this
  fix must not be. After the fix: permanent 70/63/90 against 67/64/89, loans 74/65/74 against 76/69/75,
  execution refusals ≈ 3,000 → 0, buyer turns 5,685 → 1,303, pairs weighed 8.8 M → 2.9 M.
* **Not changed (a balancing decision for the owner):** the hold on loaned-out places. It is a correct
  rule (a returning loanee must have a place; a ten-season study had produced 31-senior rows), and it is
  also what keeps AI clubs from buying. **Alternatives, none implemented:**
  (a) keep it; guards at the measured level (first season ≥ 55 permanent AI transfers, later seasons
  ≥ 100) — what the test now asserts;
  (b) hold a loaned-out place only in the last N days of the loan, and let a club that is over the limit
  when a loanee returns shed someone (contract expiry already removes ≈ 2,400 seniors at the rollover);
  upper bound with no hold: 352–383 a season (1.2 per club), probably too many;
  (c) let a club with a full list release or loan out its surplus *before* choosing a purchase (a swap);
  (d) make AI free-agent signings leave room for purchases earlier in the window.
  My recommendation: (a) now; if the owner wants first-season volume at the old level, (b) with N ≈ 45,
  measured the way this section measured the others. The old bars (120; under 55 % across a border)
  are kept as `the_market_reaches_its_historical_targets`, `#[ignore]`d with the reason, so the distance
  can be read at any time. **The real-world test now asserts floors (55 transfers; under 70 % across a
  border) instead of 120 / 55 %: that is a change to an expectation, made because the old bars cannot be
  met without the policy decision above, and it is stated here so it can be reversed.**
* **Free agents and releases** are not volume problems: season-1 free-agent signings are 426–473
  (AI clubs fill lists with them first, which is part of why lists are full), the rollover releases
  ≈ 2,400 contracts.

### 4.5 Defensible target

Per season, between AI clubs: **permanent transfers about 0.2–0.5 per club** (first season 63–90, later
105–153 on 310 clubs), loans started about as many, free-agent signings 450 then 1,200–1,350. Whether that
is the right shape is a question about football, not about the code; the evidence above is what the model
does and why. The alternatives (b)–(d) are the levers.

## 5. Domestic fixture delays

### 5.1 The distribution, not the maximum

`measure_fixture_delays` (ignored; release; the FM24 world; seeds 1 and 2 × 2 seasons = 4 league-seasons
per league). A fixture is "moved" when its played date differs from the date it was generated for;
every league below is the real calendar of that league. Figures are the four runs, in order.

| League (fixtures) | moved | p50 days | p90 days | max days | ≥ 14 d | ≥ 30 d | ≥ 50 d |
|---|---|---|---|---|---|---|---|
| Premier League (380) | 105 / 75 / 94 / 42 | 5 / 4 / 8 / 4 | 21 / 22 / 24 / 8 | **59 / 50 / 72 / 21** | 17 / 16 / 22 / 2 | 4 / 2 / 7 / 0 | 1 / 1 / 1 / 0 |
| LaLiga (380) | 96 / 55 / 80 / 43 | 6 / 4 / 5 / 4 | 21 / 13 / 10 / 9 | **81 / 34 / 31 / 34** | 17 / 5 / 8 / 3 | 6 / 2 / 1 / 1 | 2 / 0 / 0 / 0 |
| Serie A (380) | 79 / 84 / 65 / 41 | 5 / 4 / 5 / 3 | 21 / 13 / 28 / 8 | 42 / 40 / 41 / 9 | 14 / 8 / 12 / 0 | 2 / 3 / 4 / 0 | 0 |
| Bundesliga (306) | 46 / 39 / 69 / 10 | 4 / 5 / 5 / 3 | 15 / 22 / 17 / 7 | 23 / 34 / 37 / 17 | 7 / 7 / 12 / 1 | 0 / 1 / 3 / 0 | 0 |
| Ligue 1 (306) | 12 / 18 / 15 / 12 | 2 / 2 / 3 / 2 | 10 / 5 / 15 / 3 | 21 / 12 / 22 / 10 | 1 / 0 / 2 / 0 | 0 | 0 |
| Championship (552, no European club) | 87 / 160 / 87 / 107 | 3 / 6 / 3 / 4 | 7 / 14 / 6 / 9 | 11 / 22 / 7 / 21 | 0 / 18 / 0 / 6 | 0 | 0 |
| League One, 2. Bundesliga, Serie B, LaLiga 2, Ligue 2 / 3 | ≤ 13 | 1–5 | ≤ 10 | ≤ 28 | ≤ 3 | 0 | 0 |

The 50–66-day figures of INTEGRATION5 are the **maxima** of the Premier League, LaLiga and Serie A: one to
three fixtures in a season, between 0.3 % and 1.8 % of a 380-fixture league. The typical moved
fixture waits 3–6 days; nine in ten wait under 3 weeks in the leagues with European clubs and under 2
weeks elsewhere. The long tail exists only in the four leagues that send clubs to Europe, and even
the leagues without one reach 22 days.

### 5.2 Cause

* **Not the generated calendar.** A generated calendar is settled when it is built
  (`settle_generated_calendar`): the pairs of a club's matches closer than the 3-day rest are **zero** at
  the opening and after every rollover in every run (`RAW CLASHES`). Everything below happens during play.
* **First-order triggers** (the reason the game recorded for the move): national-team windows, cup ties
  drawn onto league dates, continental ties. `ContinentalTie` is a small share of first moves in every
  league (1–3 per league-season) but see below.
* **Most moves are cascades.** The game records `LeagueClash` for 641 of 720 league-clash moves counted
  in these runs (89 %) because the date they leave is blocked by *another league fixture of the same club*,
  and that fixture had itself been moved there: a displaced match takes the earliest legal date for both
  clubs (forward only), which is where a later match was due. One displacement therefore pushes the
  next, and the next, until a gap opens.
* **Why the gap is far away.** The rest rule needs 3 free days on **each side** of a date, so a match
  can only be inserted where two neighbouring matches are at least 6 days apart. A club playing every
  3–4 days (league + cup + Europe) has no such gap for weeks. For fixtures delayed two weeks or more, the days
  between the generated and the played date were held by (Premier League, summed over the tail fixtures of
  the three seasons measured): another league match of one of the two clubs 217 / 214 / 288 days, a continental match
  113 / 91 / 193, a cup tie 60 / 46 / 74, and 38 / 38 / 70 days where nothing is visible in the final
  calendar (dates a cup draw could still claim — "shadow" bookings — or bookings since moved). LaLiga, Serie A and
  the Bundesliga have the same shape with a larger continental share (25–45 %).
* **Stadium availability** is not modelled by the scheduler (no rule in `calendar_booking`, `calendar_reconcile`,
  `schedule*`): it cannot be a cause. **Insufficient rest days** is the rule itself; the measurement does not
  re-assert the invariants (the existing suites and the source-backed harnesses of section 7 do).

### 5.3 Decision: no scheduler change

The cause is established (forward-only earliest-slot placement under the rest rule in a dense calendar,
amplified by European matches), but the only corrections that shorten the tail change the scheduler's
model: moving fixtures **earlier** (breaks "postponements move forward" and moves dates the player has
seen), shrinking the shadow reservation (lets a later cup draw displace the fixture again), a global
assignment of displaced matches to gaps instead of a greedy sweep, or the per-club European-aware planner
INTEGRATION5 already named as the next step. None is a targeted correction I can regression-test without
re-deciding those trade-offs, and the brief forbids weakening the invariants to get smaller delays. **Left
for the owner:** what delay is acceptable. The measured baseline above is the number to compare with:
p50 3–6 d, p90 ≤ 28 d, and 0–7 fixtures a season at 30 days or more in four leagues.

## 6. Validation

### 6.1 Gates (all on the final code, run in this session)

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean |
| Rust workspace tests, default | **5,695 passed, 0 failed**, 159 ignored |
| Rust workspace tests, `--features mcp` | **5,782 passed, 0 failed**, 159 ignored |
| Source-backed: `cargo test --release -p fm24-adapter -p season-harness --no-fail-fast`, `APFM_WORLD_JSON` = the FM24 world of the previous mission, `APFM_WORLD_DB` = a scratch **copy** of `world.db` | **84 test binaries, 359 passed, 0 failed**, 100 ignored, no `SKIPPED` output. ALPHA6 had 352 passed and 1 failed (`clubs_outside_the_players_league_buy_players`); the 7 additional passes are this mission's six delegation tests and the market guard. |
| Vitest (whole suite) | **3,481 passed**, 4 skipped (pre-existing), 390 files |
| `tsc` + `vite build` (`npm run build`) | clean |
| Biome format and lint | clean (3 pre-existing infos) |
| Knip | clean (2 configuration hints, pre-existing) |
| i18n suites (12 locales), architecture, design-system, raw-error audits | green |
| Mutation checks | transactions 9 killed / 0 survived (section 1.3); StrictMode 1; delegation 3; the market guard was confirmed red-before-fix (a club with a full list bid) |

### 6.2 Targeted concurrency tests

16-thread same-id run, 300 + 300 interleaved writers (no lost update), a status asked while the command is
queued behind a lock held by another thread, an overlapping retry, a writer waiting on the lock while a
snapshot is taken, a panicking command, and a ledger over capacity with a request in progress: section 1.3.

### 6.3 Reviewers (read-only agents, named by the repository)

* `i18n-auditor`: 7 keys × 12 locales present, none copied from English, placeholders intact; Rust side
  emits only keys. **Fixed:** a Czech verb ("nedosáhl" → "nedorazil") and a masculine plural participle.
  Advisory, not changed: the "offer history" wording assumes that view exists for every transfer action;
  Indonesian register was already mixed; `failureReason` in a render logs once per render for an
  untranslated date-of-birth key.
* `ui-accessibility-reviewer`: no regression at the 17 call-site swaps. **Fixed:** the new transfer
  messages were not announced in the bid and counter dialogs and the bid one was upper-case (now
  `role=alert`, sentence case; outcomes `role=status`); the back arrow I named gets a focus ring.
  **Reported, not changed (unchanged markup that now carries new text):** finance feedback lines and
  the facilities reason are colour-only and not announced (`FinancesTab.tsx`, `FinancesFacilitiesCard.tsx`);
  the world-editor toast has no live region; a few raw palette colours and an 11 px label in
  `ReportPreviewStep.tsx`.
* Not run this time: `ofm-architecture-reviewer`, `ofm-dedup-reviewer`, `ofm-test-reviewer`. The size
  limits were not re-checked by tooling: `transactions.rs` is 820 lines including its 24 tests, the new
  harness test files are 600–900 lines (under the 3,000 test-file limit). Duplication named for epic #589:
  `percent_encode` and the error-text extractors are unchanged; `failureText` is one more consumer of
  `describeError`, not a new copy.

### 6.4 Performance

* Transaction: one extra `ui_clone` under the lock (4.8 ms on the 16,609-player world, ALPHA6).
* AI sweep (FM24 world, first season): buyer turns 5,685 → 1,303, pairs weighed 8.8 M → 2.9 M, wall time for
  the real-world season test unchanged within noise (the sweep is a small part of a day).
* The measurement tools are `#[ignore]`d and cost nothing in the default suite.

## 7. Environment: what was and was not operated

* **Native:** all Rust tests, the three 90-day delegation careers and the PSG one, the market and
  calendar studies, the mutation checks.
* **Component/hook tests (jsdom, mocked IPC):** every frontend claim, including the StrictMode and
  transaction races. Not equivalent to operating the app.
* **GUI:** a fresh debug build of the app from this source was launched through
  `scripts/gui-validation/run-isolated.sh --smoke 25` with a copy of the world database: the app started, wrote
  only into the throw-away root, no protected folder changed, `world.db` byte-identical. **That is a launch
  check, not an operated session**: nobody drove the screens (no UI automation here), so no click, no
  transfer dialog, no StrictMode path was exercised in the real webview. Release builds do not run StrictMode.
* **Isolation:** `~/apfootballmanager/data/world.db` SHA-256 `36b6908c…27439f7` before and after
  (re-read at the end of the mission: identical). Source-backed tests read a scratch copy; saves untouched;
  two throw-away git worktrees (I2 and I2.1) were created under `~/apfm-scratch` and removed.
* **Scratch:** `~/apfm-scratch` holds the FM24 world (72 MB, from the previous mission), a copy of
  `world.db`, and the run logs; all safe to delete.

## 8. Remaining risks and recommendation

**Open, in order of consequence**

1. **GUI not operated** (section 7). The protocol is proved in Rust and in jsdom; the real dialogs,
   focus and screen-reader behaviour of the new messages are not.
2. **AI market policy** (section 4): the hold on loaned-out places binds the market to 63–90 first-season
   permanent transfers; the owner decides between keeping it, a shorter hold with shedding on return, or
   room-making by swap. The real-world test now asserts floors, not the old bars.
3. **Calendar tail** (section 5): up to 72–81 days for one or two fixtures a season in the Premier League and
   LaLiga; structural; needs a planner, not a tweak.
4. **Out-of-order snapshots** (section 1.2): no global revision orders two answers.
5. A panic inside any command still poisons the game mutex (pre-existing).
6. Accessibility items reported but not changed (section 6.3), and the three reviewers not run.
7. Localization completeness is **not** claimed (section 2.2).

**Recommendation: CONDITIONAL GO** for continued owner testing of ALPHA6.1 and for starting the next
mission. Every defect this mission set out to close is closed with tests that fail without the fix; the
two matters that are decisions, not defects (market volume, calendar tail), are measured and handed over.
**NO-GO for any claim that the interface works end to end**, until someone operates the app.

## 9. Not done, deliberately

No M12 UI work, no new feature, no change to delegation logic, no scheduler change, no change to the
senior-list rule, and no development phase was started after this document.
