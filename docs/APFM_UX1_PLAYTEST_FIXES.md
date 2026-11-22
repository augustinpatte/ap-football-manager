# APFM UX1: fixes from the first GUI playtest

Four failures the owner reported after the first real playtest. Each section says what was wrong,
where the cause was, what changed and what was measured. Nothing here changes a match outcome,
a rating, a save format or an IPC contract.

## 1. Scouting: "Staff member is not a scout"

**Cause.** FM24 imports file a recruitment analyst under `StaffRole::Scout` and tell it apart only
by `post == Analyst`. The backend refuses that post (`ofm_core::scouting::resolve_user_scout`), but
the frontend chose scouts by `role === "Scout"` and never received `post` in its type, so the first
click on Scout sent an analyst and failed.

**Change.**

- `ofm_core::scouting::is_field_scout` is now the one backend predicate (Scout role, not the Analyst
  post). `resolve_user_scout` and the assistant's scouting delegation both use it.
- `src/lib/scoutEligibility.ts` mirrors it (`isFieldScout`, `teamFieldScouts`). Every place that
  picked scouts by role now uses it: the player profile, the scouting tab, the players list, the
  transfers actions and the youth academy tab.
- `StaffData` gained `post?: StaffPost` (the backend omits it when it can be derived, so absence means
  an ordinary scout). `StaffPost` moved to `store/types.ts`.
- The player profile now says how many scouts are free, how many exist and how many assignments are
  running (`scouting.profileAvailability`), so a disabled button is explained.

**Tests.** `scoutEligibility.test.ts` (every role/post combination an import can write),
`PlayerProfile.scouting.test.ts` (analyst-only club, analyst listed before a scout),
`PlayerProfileScoutAction.test.tsx`; Rust `only_scout_and_head_of_scouting_posts_are_field_scouts`,
`send_scout_rejects_an_analyst_filed_under_the_scout_role`, `send_scout_accepts_a_head_of_scouting`.
The frontend tests were seen failing first. The Rust tests pin behaviour that already existed.

**Contract note.** The two predicates are kept in step by hand and by tests with the same
combinations; there is no generated type. If the post list grows, change both.

## 2. Press conference asked about 0-0 after a 4-2

**Cause.** Only the classic live path keeps the page's `snapshot` current. M12 quick simulation, the
M12 replay and a managed match finish elsewhere and leave `MatchSimulation` holding the pre-match
0-0 snapshot. `PostMatchScreen` was given the real score through `spatialResult`, but
`RoundDigestScreen` and `PressConference` read `snapshot.home_score` directly, so the digest scoreline
and every conference question described a draw. The backend was never wrong: `apply_press_conference`
takes the scoreline from the game's last completed match, so the article was right and only the
questions were not.

**Change.**

- `components/match/finalResult.ts` (`settleSnapshot`) returns the snapshot the screens after full
  time must read: score from the finalised fixture, else from the response that finalised it, else
  from the snapshot; goal events from the response, else the live snapshot if its score agrees, else
  one `Goal` per stored scorer. A stale snapshot's events are never kept against a different score.
- `MatchSimulation` passes that snapshot to the post-match, digest and press screens. A managed match
  also gets its goals into the post-match report (it used to show none).
- A press conference the backend refuses (or cannot record) is no longer closed as if held. It stays
  open with the translated reason, **Try again** and **Leave without recording**. A second submit
  while one is in flight is ignored.

**Tests.** `finalResult.test.ts`, `MatchSimulation.finalResult.test.tsx` (the 4-2 versus 0-0
reproduction through the real page; seen failing first), `PressConference.test.tsx`.

**Not changed.** No extra finalisation call: the settled snapshot is a pure function of state the
page already holds. The backend's `last_completed_match` picks by date, so two completed fixtures on
one date could in principle be confused; that predates this work and is untouched.

## 3. Transfer dialogs extending below the screen

**Cause.** `Dialog` centred its panel with no height bound. A taller panel overflowed the overlay at
both ends, so its top was cut off and its action buttons sat below the window.

**Measured in headless Chrome** (real app CSS and the real components with their tallest content:
a counter-offer with projection, instalments, feedback and history), before the fix emulated by
removing the new rules:

| Dialog | Window | Before: panel spans | Submit reachable | After: panel spans | Submit and Close reachable |
|---|---|---|---|---|---|
| Bid | 960×640 | -362 to 1002 | no | 16 to 624 | yes |
| Bid | 1280×800 | -282 to 1082 | no | 16 to 784 | yes |
| Bid | 800×600 | -382 to 982 | no | 16 to 584 | yes |
| Counter-offer | 960×640 | -18 to 658 | no | 16 to 624 | yes |
| Renewal | 960×640 | -58 to 698 | no | 16 to 624 | yes |

All twelve cases (bid, counter-offer, renewal at 960×640, 1280×800, 800×600, 1024×600) are
reachable after the fix, with Submit and Close visible without scrolling. The deal workspace
(which embeds the same forms) was measured at 960×640, 1280×800 and 1024×600: the page does not
scroll and the action row is reachable.

**Change.**

- `Dialog`'s panel is `max-h-full overflow-y-auto overscroll-contain`: bound to the window, scrolling
  inside itself.
- `DialogFooter` (new, in `ui`) pins the action row to the bottom of whatever scrolls. The bid,
  counter-offer, loan and free-agent forms use it, so it also holds in the deal workspace.
- The renewal and termination modals were hand-rolled frames with no role, name, focus or Escape.
  They are now `Dialog`s (named by their heading, focus on open and restored, Escape, Tab trap);
  the termination dialog refuses Escape while a release is being submitted.
- `DashboardModalFrame` (used by the confirmation modals) is bounded and scrolls too.

**Tests.** `Dialog.test.tsx`, `PlayerProfileContractModals.test.tsx`. jsdom has no layout, so these
pin the declared contract only; the measurements above are the evidence that it holds in a browser.

## 4. Match mode confusion

**Finding.** Routing was already correct, and a test now pins it: **Manage** opens
`InteractiveMatchScreen`, **Watch** prepares a replay and opens `M12ReplayViewer`. The confusion was
the signposting. "Watch in 2D" was the most prominent option and opened a free-seek replay with no
sign that the match had already been played; "Manage the match" was the fourth card; and the big
header button silently started the text-only classic match.

**Change.**

| Mode | Label | What it is |
|---|---|---|
| Manage | **Manage Match in 2D** (first, highlighted) | A live match on the 2D pitch: tactics, substitutions, pace, resume after each stoppage. Time only moves forward. |
| Watch | **Watch simulation** | The match is simulated first, then replayed. Pause and seek, but no tactics or substitutions. Result counts at *Go to result*. |
| Quick | **Quick simulation** | Immediate result; the replay is stored. |
| Classic | **Start classic match** | Text commentary, no 2D pitch. |

- Every option's explanation is also its button's accessible description.
- The replay screen carries a note: a pending replay says the engine has already played the match
  and nothing done there changes it; a stored one says it is the replay of a completed match.
- The interactive screen says time only moves forward, and has no timeline (a test asserts it).
- Wording changed in all 12 locales (`match.startMatch`, `watchWithM12`, `viewMode.watchDesc`,
  `interactive.manage`, `interactive.manageDesc`, `replayNotRecorded`; new `replayBanner.*`,
  `interactive.liveBadge`).

**Does this need an engine change? No.** The live path is the stateful M12 `MatchSession` (F4) with
a read-only frame sampler (I6, [`APFM_I6_2D_MATCH_EXPERIENCE.md`](APFM_I6_2D_MATCH_EXPERIENCE.md)):
orders change football that has not been produced yet. The replay path is a one-shot simulation
whose frames already exist, which is why it can be sought and cannot be steered. They are two
different things and are now labelled as two different things. Gaps that remain are listed below.

**Opening frame.** Read from a real match in two ways: the interactive session's first frame and the
prepared replay's `frames[0]`. Both are the kick-off at 0 ms: ball on the centre spot (52.5, 34), eleven
a side, each side in its own half (the kicker on the halfway line), goalkeepers on their lines,
home facing +x and away facing −x. Two Rust tests pin this
(`the_opening_frame_is_a_proper_kickoff`, `a_prepared_replay_opens_on_a_proper_kickoff`); they
passed when first written, so they are characterisation, not a fix. The replay was also drawn by the
real `M12ReplayViewer` in Chrome. The owner's screenshot itself was not available, so if it showed
something else (for example a frame a few seconds in, since the replay starts playing at once), that
is not what was checked.

## Reproduced and corrected

| # | Reproduced (before) | Corrected (after) |
|---|---|---|
| 1 | `PlayerProfile.scouting.test.ts`: an analyst (role Scout, post Analyst) listed before a real scout is picked as the scout, and a club with only an analyst offers the button. | The analyst is never picked or counted, on the profile, scouting tab, players list, transfers actions, youth academy and staff page. `scoutEligibility.usage.test.ts` fails if any screen compares the Scout role itself (mutation-checked). |
| 2 | `MatchSimulation.finalResult.test.tsx`: after an M12 quick simulation that ends 4-2, the digest and the press conference receive 0-0 and no goals. | Both receive 4-2 with six goals; the same holds after a managed match whose finish returns no events (goals come from the stored fixture). Removing either wiring fails the tests (mutation-checked). |
| 3 | Headless Chrome, real CSS: the bid panel spans -362 to 1002 in a 640px window; Submit and Close are unreachable. | Panel 16 to 624, Submit and Close reachable (tables below). |
| 4 | The 2D replay opens on a bare pitch with a free-seek timeline and nothing says the match was already played; the manage option is the fourth card. | Manage Match in 2D leads; Watch simulation says what it can't do; the replay and the interactive screen each carry a note. |

## Browser measurements

Headless Chrome through the repo's own CDP helper, on a scratch page mounting the real components
on the real app CSS (not the packaged Tauri app). "Before" is the layout with the new rules removed.

Round 1, before the reviewers' fixes (reachable = on screen and not covered, at rest and after
scrolling the panel to its end):

| Dialog | Window | Before: panel spans / Submit reachable | After: panel spans / Submit and Close reachable |
|---|---|---|---|
| Bid | 960×640 | -362..1002 / no | 16..624 / yes |
| Bid | 1280×800 | -282..1082 / no | 16..784 / yes |
| Bid | 800×600 | -382..982 / no | 16..584 / yes |
| Bid | 1024×600 | -382..982 / no | 16..584 / yes |
| Counter-offer | 960×640 | -18..658 / no | 16..624 / yes |
| Counter-offer | 800×600 | -38..638 / no | 16..584 / yes |
| Renewal | 960×640 | -58..698 / no | 16..624 / yes |
| Renewal | 800×600 | -78..678 / no | 16..584 / yes |

(At 1280×800 the counter-offer and renewal already fitted before and still do.) The deal workspace,
which embeds the bid form, was measured at 960×640, 1280×800 and 1024×600: the page does not scroll
and the action row is reachable.

Round 2, after the reviewers found the pinned footer transparent in the two contract modals (the
footer took its background from a wrapper with none). Renewal, termination and counter-offer, light
and dark, at 960×640 and 1280×800 (12 cases): the footer is a direct child of the panel, its computed
background is opaque (`rgb(255,255,255)` light, `rgb(19,27,51)` dark), and every footer button is
reachable at rest and after scrolling. This measures the fix; it does not show the transparent
footer overlapping text before it, which the reviewers inferred from the code.

## Reviewer findings

The five read-only reviewers were run on `82543118..45517de4`. None found a P0.

| Reviewer | Finding | Disposition |
|---|---|---|
| accessibility P1, architecture P2 | `DialogFooter` transparent in the renewal and termination modals. | Fixed (`25274510`); structural test; Chrome round 2. |
| test P1 | Analyst eligibility unguarded on four UI surfaces. | Fixed: source guard (mutation-checked), analyst tests on the scouting tab and staff page. |
| test P1 | Profile free/active counts untested. | Fixed: computation test (busy, youth, analyst's stale assignment). |
| dedup P2 | A third copy of the predicate in `playable_journey.rs`; `StaffTab` still treated an analyst as a scout; a second definition of "free scout". | Fixed: all use the shared rule. |
| test P2 | Delegated scouting not guarded against an analyst. | Fixed: `delegated_scouting_never_sends_an_analyst`. |
| test P2 | Managed-match and post-match wiring untested at page level; press conference tests weak (reason, retry payload, double submit). | Fixed; mutation-checked. |
| accessibility P2 | Focus lost after a failed submit; highlight reused the focus-ring token; `px-4` overlay on the digest dialog. | Fixed. |
| i18n P2 | pt/pt-BR used a different word for "assignment" than their Scouting tab. | Fixed. |
| i18n P2 | pt mixes formal and familiar forms across `match.*`; id mixes "Anda" and "kamu". | Not changed: predates this work. |
| architecture P2 | The date-picker dropdown inside `FastForwardDialog` might be clipped by the dialog's new scroll area. | Not verified in a browser. The dropdown extends the scroll area rather than being cut, so it stays reachable by scrolling; open. |
| architecture note | `MatchSimulation` is about 610 lines against a 200-line ceiling. | Predates this work and grew by about 14 lines; the natural extraction is a finalisation hook; open. |
| dedup, accessibility | `DashboardModalFrame` duplicates `Dialog` without role, name, focus or Escape; nine modals still use it. | Predates this work; now bounded in height only; open. |
| accessibility P2 | Contrast of `danger` on `danger-tint`, gray-500/400 on the hero card, and the Next button's missing focus ring were not checked. | Open: needs a person in both themes. |
| test P2 | Dialog height tests assert class names only (jsdom has no layout). | Accepted; the Chrome measurements are the evidence. |
| test P2 | The opening-frame Rust tests are characterisation, not fix guards. | Accepted; labelled so in section 4. |
| test P2 | Matching English wording in `matchModeWording.test.ts` is a copy test and is brittle. | Accepted. |

One reviewer note was wrong for this repo: the claim that `simulate_spatial_match` is never called
by the frontend. It is (`matchReplayService`), and the negative assertion on it is valid.

## Verification

Code under test: `eefb949f`. The commit after it changes this file only. Frontend gates ran on
`25274510`; the only code change since is a test helper in Rust (`world_io.rs`), so they were not rerun.

| Gate | Result |
|---|---|
| `cargo fmt --check` | pass |
| Clippy `-D warnings`, default features, workspace, all targets | pass |
| Clippy `-D warnings`, `--features mcp` | pass |
| `cargo test --workspace` (default) | **5,347 passed, 0 failed**, 135 ignored (167 test targets) |
| `cargo test --workspace --features mcp` | **5,434 passed, 0 failed**, 135 ignored (167 test targets) |
| Vitest (`npm test`) | **3,272 passed, 0 failed**, 4 skipped (373 files) |
| `tsc --noEmit` | pass |
| `npm run build` | pass |
| Biome lint (`--error-on-warnings`) / format check | pass (3 pre-existing infos) / pass |
| Knip | pass (two configuration hints, as before) |
| i18n gate (`src/i18n`, `src/utils`) | pass: 13 files, 110 tests, all 12 locales |

How the results came about, because two runs did not simply pass:

- The first complete Rust run, on the code at `45517de4`, passed: 5,346 passed, 0 failed.
- The next run, after the reviewers' fixes, failed. `world_io`'s
  `a_manifest_whose_shards_do_not_parse_is_an_error_not_an_empty_world` failed with
  `worldReadFileFailed`. It is a pre-existing flaky test: its helper named its temp directory by a
  timestamp alone, so tests shared a directory and one's cleanup deleted another's file. It failed in 3
  of 8 isolated runs before any change of mine and none of my commits touch that code. The helper now
  uses a per-test name (`eefb949f`): 0 failures in 15 runs.
- The same run's MCP pass reported the `openfootmanager` bin target as failed with no output. Run
  alone, the target passes (0 tests, exit 0), and it passed in both full runs on `eefb949f`. **The cause
  of that one failure is not known.**
- One vitest run failed once in `GlobalMarketTab.test.tsx` (keyboard focus) while a Cargo build was
  running. It passed alone and in every later full run, including one on an idle machine. Treated as
  load-induced timing; not investigated further.


## GUI statement

The real Tauri application was **not** manually operated. There is no display in this environment.
Everything above is automated tests, or a headless Chrome page that mounts the real components on the
real CSS with the backend mocked. The owner's full flows (profile → Scout, a match through to the press
conference, an offer through to its counter-offer) have not been walked by a person, and the owner's
2D screenshot was not available.

## Still open

- A person must walk the four flows in the real app, in light and dark, at 960×640 and 1280×800.
- Contrast and focus-ring checks listed above (accessibility reviewer).
- `FastForwardDialog`'s date-picker dropdown inside the new scrolling panel (not seen in a browser).
- `DashboardModalFrame` confirmation modals still lack a role, a name, focus handling and Escape; nine
  files use it. Canonical home for the fix: `src/components/ui/Dialog.tsx`.
- `MatchSimulation` is over the function-size ceiling; extract a finalisation hook.
- Spectators are offered Watch and Quick simulation, not Manage, because they run no club.
- Interactive matches store no replay (so no *Replay in 2D* afterwards), as before.
- The scouting eligibility rule exists in Rust and TypeScript, kept in step by tests and a source
  guard, not generated. Candidate for epic #589: expose the rule as a flag on `StaffData`.
- The backend picks the press conference's match by latest date, so two completed fixtures on one
  date could be confused; predates this work.
- Locale register splits in pt and id (above).
