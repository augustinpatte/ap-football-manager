# APFM final functional hardening

Baseline `47507b88` (`apfm/main`), branch `hardening/final-functional-20261119`. No feature, screen or
system was added; every change below fixes a way the first playtest could go wrong, or tests one.
Nothing here was verified in the window: no person has clicked through the app, and no GUI workflow
is claimed as tested.

## Bugs found and fixed (all in the interactive 2D match screen)

| # | Defect | Effect on a player | Fix |
|---|---|---|---|
| 1 | The start effect of `useLiveMatch` depended on `t`, so a language change re-ran match start | Changing language in Settings while a match was open re-requested the kickoff frame, could show the "incomplete data" warning and rewound the picture | `useLiveMatch.ts`: `t` is read through a ref, so `call` is stable |
| 2 | An order id was `session:n` with `n` restarting at 1 on every screen. The engine remembers ids (also across a save and reload) and answers a repeated id with its first answer, applying nothing | After leaving and re-entering "Manage the match", or after Load Game, the first order given was silently swallowed as a "duplicate" of an earlier one | `useLiveMatch.ts`: every screen salts its ids |
| 3 | An order could be given while the engine was still producing a stretch (a pause pressed with a request in flight, the assistant switch, skip-ahead) | The order was refused as a stale revision, or was aimed at a moment the picture had not reached | `liveDirector.ts`: `track()` and `fetching` hold `canOrder` off, and the screen shows "Reaching the moment you can act on…", until every call of the screen has settled and its answer has been taken |
| 4 | After a reload in the second half, the director knew only the second half's stopwatch | Every first-half row of the feed and a goal banner of the first half had a blank minute | `liveDirector.ts`: the first half's clock is rebuilt from the second half's (regulation only; added time is counted on) |
| 5 | `finish()` re-armed itself after a success | A late second click asked the engine to finalise a match that no longer existed, produced an error notice and halted the director | `useLiveMatch.ts`: latched after a success, released after a failure (the match is kept and may be retried) |

Not a bug, but corrected in the earlier report: the engine's revision does move about twice per
simulated second **when frames are captured** (the 2D screen always does), and once per stretch when
they are not. Measured on the real backend; the test fake now does the same, so a stale order is refused
by the fake exactly as it would be.

## Duplications consolidated (only where they could diverge)

* "Which way does home attack": one `homeAttacksPositiveGoal(period)` in `livePlayback.ts`, used by
  `attackedGoalX`, `LivePitch` and `InteractiveMatchScreen` (it was three hand-written copies).
* "Which goal line did the goal light": one `goalLineNearest(x)`, used by `InteractiveMatchScreen` and
  `M12ReplayViewer` (it had a literal `52.5`/`105` in each).
* "Key event": the TypeScript set and Rust `is_key_event` are **different on purpose** (a highlight reel
  is cut around shots and cards; the engine stops play for substitutions, tactical changes and period
  boundaries). They were named alike; the TypeScript set is now `HIGHLIGHT_EVENTS` with a comment saying
  so. Nothing was merged.
* `matchMinute` had no caller and was removed.

## Tests added

Frontend: language change keeps the match (and does not restart it); a reopened screen's first order is
applied; no order while a stretch is in flight; first-half stopwatch after a reload; a match taken over
mid-way in a highlight mode; `finish()` once under concurrent calls, after success, and again after a
failure (`useLiveMatch.test.tsx`); the director on real half-time, order, second-half and full-time
payloads (`liveMatch.break.real.test.ts`, from `liveMatch.break.generated.json`); one rule for direction
and goal line. The engine fake in `InteractiveMatchScreen.test.tsx` now refuses a stale revision,
replays the first answer for a repeated id, bumps the revision by about 2 a second and can hold a request
in flight. One older director test asserted that orders were allowed with a request in flight; that
encoded defect 3 and was changed to the stricter rule.

Backend: an order quoted against an older revision is refused and changes nothing; a command id used
before a reload is still remembered after it; a match that ends decisively with goals is finalised with
exactly those goals (seeds scanned until it does); an academy promotion and the academy review survive a
reload; every test loop that drives a match to full time is bounded.

## Save compatibility

* Covered by automated tests: a new career saved and reopened; a match saved midway and reopened,
  finalised once; scouting dossier and shortlist round trip; academy promotion round trip; two seasons
  after a reload (`a_reloaded_career_completes_two_seasons`, run explicitly, passes).
* **Real old saves.** Seven real career saves exist in
  `~/Library/Application Support/com.apfootballmanager.app/saves`, written 28 Oct to 2 Nov by builds
  before I7 (schema v59 and v63; the current schema is v70). Three of them (one v59, two v63, the
  latest-played being 107 MB) were **copied** to a scratch folder and opened by the ignored test
  `old_saves_load_and_play_on` (`APFM_OLD_SAVES_COPY=<copy> cargo test --lib --locked
  old_saves_load_and_play_on -- --ignored --nocapture`). Each loaded and migrated to v70, held no
  dossier and no match in progress, played 60 days, played and finalised its next match once (a second
  finish refused), and saved and reloaded to an identical career. The checksums of the originals were
  taken before and after and are unchanged. The other four saves were not opened.
* Not covered: a real save written by this build *with* a dossier or a match in progress (none exists
  outside the tests).

## What remains

* P2, recorded not fixed: a match saved while watching in a highlight mode resumes at the engine's head
  (up to about 90 simulated seconds ahead of the picture that was shown); the events of that gap appear in
  the feed at once. The highlight mode itself is not remembered across a reopened screen (it starts as
  "full match"). `InteractiveMatchScreen` is still over the 200-line function limit; I7's over-ceiling
  items are unchanged; non-English translations are unreviewed by native speakers.
* The reviewers the repository's rules name (`ofm-test-reviewer`, `ofm-architecture-reviewer`,
  `ofm-dedup-reviewer`, `ui-accessibility-reviewer`) were **not run** on this diff.
* The 15-check manual pass in `APFM_MANUAL_SMOKE_TEST.md` is still the first real exercise of the GUI.
  Check 10-12 are the ones this change affects most: pause during play, orders after the pause settles,
  save and reload mid-match, and switching language with the match open.
