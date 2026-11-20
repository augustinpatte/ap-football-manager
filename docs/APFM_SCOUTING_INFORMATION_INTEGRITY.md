# Mission 2/5 — scouting information integrity (2026-11-20)

Branch `apfm/main`, from `2c894e6e` (mission 1). Nothing was pushed, reset or deleted. The serialized
`Game`/`Player` models were not changed (no migration, no new serialized field).

## 1. The rule

One projection, `ofm_core::rating_view`, decides how the manager may read a player's overall rating. It is
built on the existing knowledge rule (`player_knowledge`: own / known / scouted / unknown) and on the
scouting dossier (`scouting_dossier`), and adds no estimator of its own.

| The club's knowledge of him | Reads as | Source |
| --- | --- | --- |
| His own player (incl. loaned in or out) | **exact** rating | `player_knowledge::Own` |
| A player at a club in his division | **exact** rating (his staff see him every season) | `player_knowledge::Known` |
| A player a scout reported on | the scout's **ability range**, with confidence, report date and `current` (false once the report predates the season) | `scouting_dossier` report, freshness by `current_report_on` |
| Anyone else | a **public standing**: limited / average / good / strong / elite (62, 70, 77, 81 cut-offs: the game's S/A/B tiers plus one lower step) | `PublicStanding` |

Not refreshed silently: a stale report keeps the range it was written with and is marked *old* however the
player has changed. Potential and attributes are never given for an unscouted player. Sort and filter read
the projection: a standing sorts at the middle of its span (two players in one standing keep name order), and
a rating bound keeps whoever *could* be inside it, so neither can rank or probe a hidden rating. The squad
model is asked about a target only at a rating the club may read: a player known by standing alone has no
"would start / cover" verdict (it would have needed his true rating).

Internal simulation, AI clubs, the save and the engine keep reading `Player::ovr`/`potential`; nothing in
`ofm_core` outside the screen projections was switched to the projection.

## 2. Audited surfaces

"Leak" = an exact external rating, potential or hidden attribute recoverable by a human before scouting.

| # | Surface | What it showed | Now |
| --- | --- | --- | --- |
| 1 | Player profile hero card (OVR tile) | exact `ovr` of any player — **the confirmed finding** | `get_player_ratings` → `ReadableRating` (exact / range, old-marked / standing) |
| 2 | Profile hero + contract card: condition and morale | exact values of any external player (a scout report shows them, so they are scouted information) | shown only for a player the club knows exactly; otherwise a dash |
| 3 | Profile attributes card | already hidden unless own or scouted | unchanged (scout's noisy snapshot, marked out of date by the backend) |
| 4 | Players list: rating column, rating sort, OVR bounds | `PlayerSummary.ovr`; sort and bounds on the true rating — a bound probe finds any rating | `PlayerSummary.rating`; sort and bounds on the readable rating; header hint |
| 5 | Transfers market, my list, offers: rating column and OVR sort | exact `ovr`; backend sort by true rating | rating from the page the backend answered; sort by readable rating; header hint |
| 6 | Deal workspace facts (bid, loan, free agent) | exact `ovr` | `ReadableRating` via `usePlayerRatings` |
| 7 | Global Market (external registry) results | exact `ovr` **and** `potential`; sort by true rating | `standing` only; SQL sort by standing |
| 8 | Global Market profile | exact `ovr`, `potential` **and the full attribute block** of players nobody scouted | standing + "not scouted" note; no potential, no attributes (the recruit/negotiate commands still load the full record in Rust) |
| 9 | Scouting dossier table, report dialog, comparison, shortlist | `ScoutingTargetView.ovr` ("public rating"); `squad_impact` computed from the true rating of an unscouted player | `rating` (range or standing); `squad_impact` is `None` for an unscouted player ("unknown") |
| 10 | Scouting search card | no rating column; ability/potential ranges from the dossier | unchanged (sorting by rating now orders by standing) |
| 11 | Team profile roster of another club | "??" for everyone, ignoring division and reports | the shared projection: exact in his division, scout ranges, standings |
| 12 | Pre-match opponent scout panel | exact `ovr` and key attributes of every opposing player | the readable rating (number / range / dash); key stats only for a player known exactly |
| 13 | Inbox: scout's youth recruitment report | exact `ovr`, `potential`, growth bar and tier letter of each proposed prospect | `get_youth_prospect_readings`: standings of rating and ceiling until the club signs him (then exact); growth shown as unknown |
| 14 | MCP `scout_get_dossier` row | `public_rating` exact | `rating` string in the same shape the screen shows (number, range or standing) |
| 15 | Own-player screens (squad, tactics, academy review, own line-up, home, substitutions) | own ratings | unchanged by design |
| 16 | Old scouting report card (`ScoutPlayerCard`) | scout-derived keys and approximate rating | unchanged: already the scout's reading |

## 3. Deliberate exceptions

* **The serialized `Game` still carries every true value** (pre-existing, documented in I7). A person with
  developer tools can read it; screens never do. Closing it means stripping the wire for the whole game
  state, which is the "widespread high-risk change" the brief excludes. The guarantee is for every
  player-facing surface and page API, not for the raw game blob.
* **Team-level averages** (`avg_ovr` in the teams directory and the team profile). A club's overall strength is
  a public club-level figure, not a player's value.
* **Market value, wage, contract, age, club, position, nationality** stay public: they are what a market shows.
  Value is derived from the rating in the generator, so it is a coarse public signal by design.
* **Division rivals read exactly** (the existing `Known` rule): their staff see them every season.
* **Pre-match opponent condition** is still shown (match-day news, not a scouted attribute).
* **MCP tools other than the dossier** (squad and market listings) still give the agent exact ratings. The MCP
  reader is an AI agent, not the human player; the brief limits removal to player-visible leakage.
* **Market value is derived from rating and potential** in the registry generator (`estimate_market_value`),
  so sorting a market by value is a coarse proxy for rating. Value is a public market figure and stays; the
  rating sort itself no longer ranks inside a standing.
* **Other MCP paths** (`market` listings in `tools_impl/transfers.rs`, `PlayerDetail::Other` in
  `tools_impl/info.rs`) still return `ovr` to the agent; the dossier tool was the only one that shared the
  UI's projection. Intended: the MCP reader is an AI agent.
* **Youth messages still carry the generator's true `ovr`/`potential`** inside `MessageContext.youth_prospects`
  on the wire (needed for the sign step); the card reads `get_youth_prospect_readings` instead.
* **A pre-I7 save's attribute snapshot** with no dossier report reads as a standing, not a range (there is no
  range to show); the profile keeps showing the snapshot.

## 4. Scouting stays actionable

* A scouted player gets a range with confidence and the old-report mark; the dossier, comparison, shortlist,
  "make a bid" / "offer a contract" and "scout again" are unchanged. Scouting is *more* valuable: it is the
  only way to turn a standing into a range, and the potential filter and the squad verdict need it.
* Unscouted players are fully usable: identity, age, club, position, value, wage, contract, standing, search,
  filters (position, age, value, standing bounds), shortlist, bid, loan and free-agent flows are unchanged. The
  Global Market signing flow loads the full record in Rust.
* The accuracy model is untouched: `scouting_dossier` widths by judging, intersection of same-season reports,
  staff effects. Only the presentation of its output changed.

## 5. Tests

Written to fail first where a failing run was seen (compile or assertion); the rest were checked by
**mutation** (the fix removed, the named test seen failing, restored):

| Mutation | Test that fails |
| --- | --- |
| players-page rating sort reads the true `ovr` | `sorting_unscouted_players_in_one_standing_reveals_no_ranking`, `exact_ranges_and_standings_sort_by_what_the_manager_reads` |
| ovr bounds read the true `ovr` | `a_rating_filter_cannot_probe_an_unscouted_players_true_rating` |
| squad verdict asked at the true `ovr` | `an_unscouted_target_shows_no_rating_and_no_squad_verdict`, `the_squad_verdict_of_an_unscouted_target_does_not_follow_his_true_rating` |
| registry SQL sorts by `ovr` | `every_sort_direction_orders_the_page_the_screen_asks_for` |
| market row prints `player.ovr` | `TransfersTab.ratings.test.tsx` (both) |
| hero shows condition for an unscouted player | `PlayerProfile.test.tsx` and `PlayerProfileHeroCard.test.tsx` |

Scenarios: unscouted (`rating_view`, players slice, dossier, registry, every screen), partially scouted (low
confidence), high-confidence, old report (range kept, marked old, even after the rating changed), own and
division players, legacy snapshot, youth prospect before and after signing, and save/reload (a serde round trip
in `rating_view`, and `db/tests/rating_view_reload_tests.rs`: a real `.db` file, fresh `SaveManager`, an old
report still old and unchanged, true ratings still in the save).

A reload test of mine first failed on a fixture mistake worth recording: the season start used by "old
report" is derived from the league's fixtures on load, not stored, and a legacy single league becomes a
competition on load. The test now builds a league and keeps the strangers outside it.

## 6. Reviewers (run on `2c894e6e..HEAD`) and what was done

| Reviewer | Result | Action |
| --- | --- | --- |
| `i18n-auditor` | no defects (21 keys x 12 locales, placeholder parity, no English copies) | advisory on spoken dashes taken (below) |
| `ui-accessibility-reviewer` | 5 findings: the inbox Potential badge was spoken as "Overall rating"; bare ranges/dashes in the pre-match token and list had no spoken text; the sort caveat only in `title`; unknown condition/morale a bare dash; the dialog lost its visible "Rating" word | fixed: `subject="potential"` variant with its own spoken sentence and hint, `SpokenRatingFigure` and a token `ovrDescription`, sort caveat as `sr-only` text in the headers, "not known" for unknown dashes, visible "Rating:" label. Contrast of `text-warning` / `text-fg-subtle` on `bg-sunken` was **not measured** (needs eyes) |
| `ofm-architecture-reviewer` | no reachable failure; `Viewer`'s `RefCell` memo is sound (never stored, never crosses a thread or an await); AI/simulation paths still read true `ovr` | a production `HashMap` index in the sort replaced by `get`; an orphaned test doc comment restored; the MCP and market-value observations recorded above |
| `ofm-test-reviewer` | 6 gaps: SQL rank only half covered, confidence labels only "moderate", estimate wire shape unpinned, estimate rows not filter-tested, some negatives that could not fail, a leaking mock | added: SQL rank for every rating 1 to 99 against `PublicStanding`, all three confidence levels, estimate and prospect JSON pinned in the adapter test, an estimate-row bound test and a `potential_min` test, text-content negatives in the inbox test, a mock reset in the pre-match test. The `GlobalMarketTab` negatives are weak by construction (the types no longer carry the fields); the Rust wire tests are the protection |
| `ofm-dedup-reviewer` | the range formatter existed three times; two similar fetch hooks; hand-built Rust fixtures; a comment gap on the harness copy of the cut-offs | `formatRange` moved to `src/lib/ratingView.ts` (the dossier re-uses it); harness copy now says why and when it goes; a comment on `RegistryPublicDetail`. **Not done**: a shared keyed-fetch hook (the shared logic is one flag) and reusing `market_fixtures` in the new integration test (low value) |

## 7. Validation and final state

See the end of this file.

### Final run (code state `104fa882`; this document is committed after it)

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 5281 passed, 0 failed (133 ignored: need a display or the nightly) |
| `cargo clippy --locked --workspace --all-targets -- -D warnings`, default and `--features mcp` | clean |
| `cargo fmt --check` | clean |
| `npm test` | 3209 passed, 4 skipped, 0 failed (includes `src/i18n`, `architecture.test.ts`, `designSystem.test.ts`) |
| `npm run format:check`, `npm run lint`, `npm run knip`, `npm run build` | clean (lint reports 3 pre-existing infos) |
| Mutation checks | 6 of 6 caught (section 5) |
| Reviewers | 5 of 5 run (section 6) |

**Not verified:** the real desktop window was not operated (no computer-use access in this session), so no
screen was seen by eye, no screen reader was used, and contrast of the new tokens in both themes is
unmeasured. `npm run audit:i18n` was not run (advisory, never fails); the real i18n gate passes.
Commits are local; nothing was pushed.
