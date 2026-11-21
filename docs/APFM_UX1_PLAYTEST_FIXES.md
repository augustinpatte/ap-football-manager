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

## Verification

Run: `tsc`, Biome lint and format, knip, the full vitest suite, `cargo fmt --check`, clippy at
`-D warnings` (default features) and the Rust workspace tests. Not run: the read-only reviewer agents
(`ofm-architecture-reviewer`, `i18n-auditor`, `ofm-test-reviewer`, `ui-accessibility-reviewer`,
`ofm-dedup-reviewer`), clippy with `--features mcp`, and nothing was clicked through in the real
Tauri app (no display here). The Chrome measurements use the
app's own CSS and components on a scratch page, not the packaged app.

## Still open

- No GUI session of the real app: the owner's full flow (profile → Scout, a match to the press
  conference, an offer through to its counter) has not been walked end to end by a person.
- `DashboardModalFrame` confirmation modals are bounded but still lack a role and a name; only the
  two contract modals were moved onto `Dialog`.
- Spectators are offered Watch and Quick simulation, not Manage, because they run no club.
- Interactive matches store no replay (so no *Replay in 2D* afterwards), as before.
- The scouting eligibility rule exists twice (Rust and TypeScript) and is kept in step by tests,
  not by generation.
