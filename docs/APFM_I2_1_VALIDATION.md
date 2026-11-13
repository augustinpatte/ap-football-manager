# APFM I2.1: validation

Evidence and its limits for [`APFM_I2_1_TRANSFER_INTEGRITY.md`](APFM_I2_1_TRANSFER_INTEGRITY.md).
Branch `deepseek/i2-1-transfer-integrity`, based on `73db8d43`. Nothing pushed or merged.
Host: macOS, Apple Silicon. Labels: **PASSED**, **FAILED**, **NOT RUN**. A local pass is not a
claim about CI.

## 1. Verdict

**GO** for the transfer-integrity scope.

The promised senior place is reserved on every route that signs a senior — in-game bid,
registration, free-agent talk (offer and completion), global-market free agent and purchase, and
AI squad planning — and is released by the existing status transitions. Boundary tests exercise
29/30 seniors with and without a promise, a place freeing, a withdrawal and a save round trip
through production entry points. The free agent's signing bonus is now judged with the pending fees
of agreed deals. The legacy external preview delegates to the real negotiation. The full Rust and
frontend suites, `cargo fmt --check`, Clippy at `-D warnings` with and without the MCP feature, and
the production build pass. Three ten-season generated-world careers and three imported-world
careers finish with clean invariants, no mutating refusal and no place above 30.

Why not "GO" without qualification: a pending **loan-in** still reserves no senior place, and a
mid-talk squad-limit failure reuses the money-flavoured `ClubCannotAfford` reason. Both are named
in the integration doc §9 and neither is caused by this change.

## 2. Tree and scope

| Item | Value |
| --- | --- |
| Base | `73db8d43` (`docs(i2): integration and validation reports`) |
| Branch | `deepseek/i2-1-transfer-integrity` |
| Commit | `<commit>` |
| Files changed | 11 modified, 1 new test module, 2 new docs; no frontend, translation, engine, M12, formation-AI or tactical file |

## 3. Gates run on the final tree

| Gate | Command | Result |
| --- | --- | --- |
| Rust format | `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | **PASSED** |
| Clippy | `cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings` | **PASSED** |
| Clippy, MCP | same with `--features mcp` | **PASSED** |
| Workspace tests | `cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace --no-fail-fast` | **PASSED**: 5,053 passed, 0 failed, 121 ignored — 152 result lines, all `ok`, across 143 test binaries and 9 doc-test targets |
| Library tests, MCP | `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --features mcp` | **PASSED**: 673 passed, 0 failed, 10 ignored |
| Frontend suite | `npm test` | **PASSED**: 343 files passed, 1 skipped; 3,012 tests passed, 4 skipped |
| Type check + build | `npm run build` | **PASSED** |

`--locked` was used for the Rust gates. The only dependency override is the existing pinned M12
revision; no dependency changed.

### Not run
`cargo deny`, `cargo machete`, `npm run lint`/`format:check` as separate gates, `/preflight` as one
invocation, CI, any desktop GUI step, and a 30-season career. Frontend files were not touched, so
the frontend gates are reported from the pre-doc tree, not re-derived from a frontend change.

## 4. Red then green

Each fix that changed behaviour was seen to fail first, then pass, with the production code
reverted and the tests kept. The table names the test that failed for the predicted reason and,
where a row is a guard rather than a reproduced red run, says so.

| Fix | Red evidence | Green |
| --- | --- | --- |
| Free-agent talk reserves the promised place | With the production files at `73db8d43`, `a_free_agent_offer_is_refused_when_the_last_place_is_promised_to_a_purchase` and `a_free_agent_lapses_when_the_last_place_is_promised_before_he_answers` opened a talk / signed the player instead of refusing. | Both pass with `ensure_room_for`. |
| Free agent's bonus counts pending fees | With the old `ensure_bonus_affordable`, `a_free_agent_bonus_is_judged_against_the_fees_of_agreed_deals` opened the talk on raw cash, although the pending fee and the bonus together overdraw the club. | The offer is refused with `be.error.transfers.insufficientFunds`; withdrawal of the deal releases the fee and the same offer is taken. |
| Global-market free agent respects the promise | With the old `squad_blocker(free_agent = 0)`, `a_global_market_free_agent_cannot_take_the_place_an_agreed_purchase_was_promised` signed the free agent and the agreed purchase later failed. The `a_full_list...` and `a_global_market_free_agent_signs_once...` tests also failed on the old code. | All three pass; the preview blocker and the signing refusal agree and a refusal mutates nothing. |
| AI clubs respect the promise | With the gate temporarily removed from `sign_free_agent_for`, `a_club_with_a_promised_place_signs_no_free_agent` failed with `left: Some(Signed("keeper"))`, `right: None`. | The gate restored: `None`; the control club with a free place signs the same keeper. The 30-senior guard `a_full_club_signs_no_free_agent` was added for the matrix and is green on the final tree; it was not part of that red run. |
| Preview delegates to the negotiation | By inspection of the removed preview at `73db8d43`, it never ran `player_response`, so it could not report `ERR_PLAYER_REFUSES_MOVE` while `recruit_external_player` did. This is a code-reading proof, not a run: the new test was added as the guard. | The preview is `negotiate_external_transfer` with no mutation; `the_legacy_preview_agrees_with_the_attempt_when_the_player_refuses_the_move` sees both report the same key. |

The in-game bid tests and the `respond_to_offer` / `counter_offer` command tests pass on
`73db8d43` too: I2 already applied the limit at in-game agreement. They are regression guards for
the shared predicate and the self-exception, not a red-then-green claim. The command tests are the
first boundary tests of those two routes (I2 §6 item 3).

## 5. Boundary matrix: named evidence

All tests call production entry points; none constructs a "signable" state by calling the predicate
under test.

| Boundary | Test |
| --- | --- |
| 29 seniors, no promise, bid | `transfers_tests::squad_places::a_purchase_can_be_agreed_into_the_last_free_place` |
| 29, one promised purchase, second bid | `...::a_purchase_is_refused_when_the_last_place_is_promised` |
| 30 plus a pending purchase, bid (no double count) | `...::a_full_list_with_a_pending_purchase_never_exceeds_the_limit` |
| 29, own pending deal registers | `...::a_pending_purchase_does_not_block_its_own_registration` |
| 29, a senior leaves, pending deal registers | `...::a_pending_purchase_registers_once_a_place_frees` |
| 29, one promise, free-agent offer refused | `...::a_free_agent_talk_is_refused_when_the_last_place_is_promised` |
| 28, one promise, free-agent offer accepted | `...::a_free_agent_talk_is_accepted_once_a_place_frees` |
| Withdrawal releases the place | `...::a_withdrawn_pending_purchase_releases_its_place` |
| Save/reload keeps the reservation | `...::a_promised_place_survives_a_save_and_reload` |
| Global-market free agent refused | `transfers_tests::external::a_global_market_free_agent_cannot_take_the_place_an_agreed_purchase_was_promised` |
| Full list + pending purchase, global-market free agent | `...::a_full_list_with_a_pending_purchase_refuses_a_global_market_free_agent` |
| Preview and attempt agree; place freed | `...::a_global_market_free_agent_signs_once_the_promised_place_frees` |
| Legacy preview agrees with the attempt on a player refusal | `...::the_legacy_preview_agrees_with_the_attempt_when_the_player_refuses_the_move` |
| Global-market contracted purchase respects an in-game promise | `...::an_agreed_in_game_purchase_holds_its_place_against_a_global_market_purchase` (I2) |
| Free-agent offer refused at submission (real support world) | `free_agent_talks_tests::a_free_agent_offer_is_refused_when_the_last_place_is_promised_to_a_purchase` |
| Mid-talk lapse when the place is promised | `...::a_free_agent_lapses_when_the_last_place_is_promised_before_he_answers` |
| Bonus counts pending fees; withdrawal releases | `...::a_free_agent_bonus_is_judged_against_the_fees_of_agreed_deals` |
| AI club with a promise signs nobody | `squad_floor::tests::a_club_with_a_promised_place_signs_no_free_agent` |
| AI club already at 30 signs nobody | `squad_floor::tests::a_full_club_signs_no_free_agent` |
| AI club with a free place signs | `squad_floor::tests::a_club_with_a_free_place_signs_a_free_agent` |
| `respond_to_offer` refusal is non-mutating | `commands::transfers::tests::respond_to_offer_internal_refuses_a_buyer_whose_last_place_is_promised` |
| `counter_offer` refusal is non-mutating | `...::counter_offer_internal_refuses_a_buyer_whose_last_place_is_promised` |
| A rejected counter still records the rejection with no place | `...::counter_offer_internal_rejects_a_fee_even_when_the_buyer_has_no_place` |

The new module is registered in `src-tauri/crates/ofm_core/tests/transfers_tests.rs`; it is not an
orphan file, and each test was run by name in the workspace selection.

## 6. Careers

### 6.1 Generated world, three seeds, ten seasons, invariants on (RUN)

```text
P45_WORLD=gate P45_SEEDS=1,2,3 P45_SEASONS=10 \
  cargo test --release --locked --manifest-path src-tauri/Cargo.toml \
  -p season-harness --test p45_long_career p45_long_career \
  -- --ignored --nocapture --test-threads=1
```

The clean run finished in **145.27 s** (three seeds, about 48.4 s per seed) with all three
`invariants: clean`. The repeat used for the table below ran while the workspace suite and the
imported-world careers were also on the machine, so its per-seed seconds are loaded, not
comparable:

| Seed | Seasons | Invariants | Largest squad seen | Clubs in debt | Seed time (loaded repeat) |
| --- | --- | --- | --- | --- | --- |
| 1 | 10 | clean | 30 | 0 at every reported season | 52.5 s |
| 2 | 10 | clean | 30 | 0 at every reported season | 54.3 s |
| 3 | 10 | clean | 30 | 0 at every reported season | 58.4 s |

Squads are reported as `[min-max]` by `squad_study`; every season's maximum was at most 30, and
the world-health lines show 0/80 clubs in debt. The reports' exact rows are **not identical to
I2**, and should not be: AI clubs with a place promised to a purchase no longer sign a free agent
into it. The comparison that matters is integrity and world health, not cell equality: no list
above 30, no debt, no invariant violation, and a comparable run time.

### 6.2 Imported-world combined career (RUN)

```text
I1_WORLD=gate I1_SEED=1 I1_SEASONS=10 \
I1_WORLD_DB=$HOME/apfootballmanager/data/world.db \
  cargo test --release --locked --manifest-path src-tauri/Cargo.toml --lib \
  commands::registry::combined_career::i1_combined_career -- --ignored --nocapture
```

Registry: **81,762 stored, 69,301 eligible, 9,843 active-world** (built from the installed
`world.db`, never committed).

| Seed | Seasons | Free agents / contracted signed | Fees | Refusals that mutated | Ledger / duplicates | Invariants | Reload twin | Test time |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 10 | 60 / 8 | 42.47 M | 0 | 0 / 0 | clean | identical, same market | 94.6 s |
| 2 | 10 | 34 / 8 | 62.11 M | 0 | 0 / 0 | clean | identical, same market | 122.5 s |
| 3 | 10 | 34 / 8 | 27.55 M | 0 | 0 / 0 | clean | identical, same market | 110.7 s |

Refusals by key (seeds 1/2/3): boardWagePolicy 1,130 / 1,356 / 1,279;
transferBudgetTooLow 281 / 182 / 331; insufficientFunds 104 / 101 / 120; transferWindowClosed
97 / 95 / 107; negotiationNotAgreed 6 / 10 / 5; playerRefusesMove 0 / 0 / 2; **no `squadFull`**
(the study's club keeps 10-20 seniors, as in I2). The market stayed at about 66.6 k players
(17.8-18.9 k free agents), 0/80 clubs in debt every season, 0 duplicate identities, and every
reload twin differed on no line (the squad-floor journal excluded), sixty days on.

Against I2: seed 1 61/8, 38.8 M, 112 s; seed 2 33/8, 55.9 M, 102 s; seed 3 34/8, 27.3 M, 101 s.
The signings and fees are close; the exact decisions are not identical, which is the intended
effect of the AI reservation gate.

## 7. Performance

The clean three-seed, ten-season P45 test took **145.27 s** with invariants on (about 48.4 s per
seed), against I2's recorded roughly 45 s per seed: about +8 %, on a changed simulation. A repeat
taken while the workspace suite and the imported-world careers were also running took 165.43 s
(52.5 / 54.3 / 58.4 s), which shows the loaded-machine noise but is not used for the comparison.

The reservation is one linear scan of the player list (and each player's offers) per signing
attempt, plus one before an AI club's market scan; it is not called per match minute, per day or
per render, and it allocates no collection. No instruction-count measurement was run for this
phase; the wall-time comparison is the evidence, and the career output is not expected to be
identical to I2 because AI clubs with a promised place no longer sign a free agent.

## 8. Invariants checked

Each career was watched by `season_harness::invariants` (no overflow, no negative budget, no
duplicate identity, no invalid squad/registration state, no league ladder violation) and by the
squad, finance and world-health reports. All six runs reported clean invariants; the generated
world reported 0/80 clubs in debt at every snapshot and a squad maximum of 30; the imported world
reported 0 mutating refusals, 0 ledger mismatches, 0 duplicate identities, the user's club between
10 and 20 seniors, and an identical reload twin. The boundary tests additionally assert
that a refusal leaves the serialized career unchanged, that no list exceeds 30, that the promised
purchase is still `PendingRegistration` after a refused free agent, and that a withdrawn deal no
longer matches the reservation.

## 9. Reviewer-style checks applied

| Check | Result |
| --- | --- |
| Layering: `engine` never imports `domain`; no engine file changed | clean by scope (zero engine diff) |
| One authoritative rule: one capacity predicate, one pending-purchase source | `signing_place_refusal` / `pending_incoming_purchases` only; no second reservation was added |
| No `unwrap`/`expect`/`panic!` in production Rust | new production code returns `Option`/`Result`; test-only helpers may unwrap |
| No new translation key outside all locales | no key added; existing `squadFull` and `ClubCannotAfford` reused |
| Serde compatibility | no serialized field added or removed; no migration |
| Dead code | `fee_block_after_pending` removed with its re-export |

These are self-checks, not substitute reviewer-agent verdicts; no reviewer agent was run in this
session.

## 10. Not run / residual risk

* A pending loan-in still takes no place, and loans/academy promotions are still ungated (integration §9 item 1).
* A mid-talk squad-limit lapse reuses `ClubCannotAfford` (integration §9 item 2).
* No production cancel command exists for a `PendingRegistration` purchase; the reservation is
  correct for the statuses that do exist (integration §9 item 5).
* `cargo deny`, `cargo machete`, `/preflight`, CI and any GUI step were not run.
* The imported-world career was run on the installed `world.db` (240 clubs) on this machine only.
