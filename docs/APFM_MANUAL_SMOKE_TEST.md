# AP Football Manager — manual smoke test (20-30 minutes)

Do the checks in order; each one builds on the previous. Start the game as described in
[`APFM_PLAYABLE_BUILD.md`](APFM_PLAYABLE_BUILD.md). Mark each check **pass**, **fail** or **skipped**
and write one line for anything that is not a clean pass.

| # | Do this | You should see | Result |
|---|---|---|---|
| 1 | Run `npm run tauri dev` and wait for the window. | The window "AP Football Manager" with the main menu (New Game, Load Game, Settings, Exit Game). No error banner. | |
| 2 | New Game → create a manager → choose the first world → Start Career → pick a club. | The Home dashboard with your club, next match and league position. (World creation takes a few seconds.) | |
| 3 | Open **Squad**, then **Tactics**, then **Training**. | Each page lists your players or settings, with no blank page or error. Change the formation once and see the eleven update. | |
| 4 | Open **Scouting**. Search for any player, press *Scout*, choose a scout. | The assignment appears under active assignments with "days left". If you have no scout: Staff → hire one, then retry (note it, that is a finding). | |
| 5 | Press **Continue** repeatedly until a scouting report arrives (Inbox or the dossier section of Scouting). | A report with **ranges** ("62-70") and some "unknown" values, never one exact number for ability or potential. | |
| 6 | In the dossier, add that player to the shortlist, add a second one, select both and press **Compare**. | Both appear in the shortlist; the comparison shows both side by side and closes cleanly. | |
| 7 | Open **Youth Academy**. | A review card lists prospects with verdicts (ready / keep / unlikely). If a prospect is ready, press **Promote**: they appear in the Squad. | |
| 8 | Press **Continue** until the **Match Day** panel appears. Use *Auto-Select Best XI* if offered. | "Your fixture is ready to play" with the match options. | |
| 9 | Choose **Manage the match**, press **Play**. | A pitch with 11 players a side, a score and a clock that runs. Players move; the ball moves. | |
| 10 | Change the speed (try 4x), switch highlight mode (Key moments → Full match), press **Pause**, then **Play** again. | The picture speeds up and slows down; pausing stops it; the clock and score stay consistent after resuming. | |
| 11 | Pause, make one **substitution** and one **tactics change**. Play on. | The panels switch on only after the pause settles; the change appears in the feed and the new player is on the pitch afterwards. | |
| 12 | **Save game** during the match, quit to the main menu (or close and restart the app), **Load Game**, press **Continue** until Match Day and choose *Manage the match* again. | The match is waiting at the instant you saved, with the same score and clock. | |
| 13 | Play to **half time** (use *Skip to the next break* if you like) → *Start second half* → play to full time → **Finish match**. Click Finish only once. | The dashboard returns. Schedule and the league table show the result **once**; your players' stats changed. | |
| 14 | **Save game**, quit, restart, **Load Game**. Check Scouting (reports, shortlist) and Youth Academy. | Everything is exactly as you left it. | |
| 15 | Press **Continue** until the season ends and the next season starts (use quick simulation for matches to speed up). | An end-of-season screen, then a new season with a new fixture list and an empty league table. Your squad has between 15 and 30 senior players. | |

## Extra checks if time remains

* Try to promote a player when your squad has 30 seniors: the game must refuse with a message.
* In the match, try to change something **while playing** (without pausing): the controls should be
  disabled, with a note to pause first.
* Switch the language in Settings and open Scouting and the match screen: no raw keys such as
  `scouting.dossier.xyz` on screen.

## Reporting

For every **fail**, follow "Reporting a problem" in the playable-build guide: steps, screenshot, the last
100 lines of the log, and the save folder if it is career-specific.
