# AP Football Manager — playable build

How to start the game and play a first career. No development knowledge needed beyond one command.
When something goes wrong, see "Reporting a problem" at the end. The 20-30 minute test pass is in
[`APFM_MANUAL_SMOKE_TEST.md`](APFM_MANUAL_SMOKE_TEST.md).

## 1. One-time setup (macOS)

You need three things installed. If `node -v` and `cargo -V` both print a version, skip this section.

1. **Xcode command-line tools**: `xcode-select --install`
2. **Rust**: <https://rustup.rs> (the repository pins the right version itself)
3. **Node.js 18 or newer**: <https://nodejs.org>

## 2. Start the game

```bash
cd ~/apfm-migration/apfm-main
npm ci            # first time only (about a minute)
npm run tauri dev
```

* The **first start compiles the game: expect 5-15 minutes** and a lot of fan noise. Later starts take
  well under a minute.
* A window called **AP Football Manager** opens by itself. Leave the terminal open: closing it closes
  the game. Stop the game with the window's close button, then `Ctrl+C` in the terminal.
* If the terminal says a port is already in use, another copy is running: close it first.

Use `npm run tauri dev`, not `npx`. (`npx` can silently run the wrong tool.)

**Without compiling: the release app.** A release build is produced by
`npm run tauri build -- --bundles app` (about 5 minutes) and opens with
`open "$HOME/apfm-migration/apfm-main/src-tauri/target/release/bundle/macos/AP Football Manager.app"`. It is
ad-hoc signed only, so it opens from where it was built but a copy sent to another Mac will be blocked by
Gatekeeper. It uses the same saves folder as `npm run tauri dev`. Details and the validation results:
[`APFM_ALPHA_RELEASE_CANDIDATE.md`](APFM_ALPHA_RELEASE_CANDIDATE.md).

## 3. What you should see first

The **main menu**: *New Game*, *Load Game*, *Settings*, *Exit Game*. The window title reads
"AP Football Manager". There should be no error banner. If the window stays white for more than a
minute, see "Reporting a problem".

## 4. Start or load a career

**New career**

1. *New Game* → create your manager (name, date of birth, nationality) → *Choose World*.
2. Pick the generated world listed first, leave the start options alone, press *Start Career*.
   World creation takes a few seconds ("Creating World...").
3. Pick a club on the team-selection screen and confirm. You land on the **Home** dashboard.

**Load a career**: main menu → *Load Game* → click the save.

## 5. Reach the next match

On the Home dashboard press **Continue** (top right). It advances the calendar a day at a time and stops
when something needs you (a message, a transfer, or your match).

Before a match, open **Squad** and **Tactics** to check your eleven and formation (the *Auto-Select
Best XI* button in the pre-match screen is a safe default). **Training** is under Club in the left menu.

When the day of your match arrives a **Match Day** panel appears: "Your fixture is ready to play".

## 6. Play the 2D match

1. In the Match Day panel choose **Manage the match** ("Watch the match live on a 2D pitch and make the
   tactical and substitution decisions yourself"). The other choices (quick simulation, watch in 2D
   replay) also work but give no control.
2. A pitch appears with both teams. Press **Play**.
3. Controls: **Pause / Play**, speed **1x to 16x**, highlight mode (**Key moments**, **Extended**,
   **Full match**), **Skip to the next break**, and the assistant switch.
4. To change something, press **Pause**. The pitch runs on for a few seconds to the match's current
   moment, then the Tactics and Substitution panels switch on. Your orders apply from the moment shown.
5. At half time the match stops. Press **Start second half**.
6. At full time press **Finish match**. This records the result **once**. After that you are back on the
   dashboard; the Schedule, the league table and your players' statistics show the match.

You cannot give orders while the match is playing. Pausing first is intentional.

## 7. Scouting and the academy

* **Scouting** (left menu, Club section): your scouts and a search for players.
  1. Search for a player, press *Scout* on the row and choose a free scout.
  2. A scout takes a few days. The report arrives in your **Inbox** and in the *dossier* section of the
     Scouting tab.
  3. A report gives **ranges, not exact values** ("ability 62-70"), and "unknown" where the scout could not
     tell. A better scout gives a narrower range; a second report on the same player narrows it further.
  4. Add players to the shortlist from the report, select two or more to *Compare*, open a row for details.
* **Youth Academy** (left menu): the review card says which prospects are ready, which should stay and
  which are unlikely to progress. *Promote* moves an eligible player to the senior squad. A club may have
  at most **30 seniors**; promotion is refused with a message at 30. The game tries to keep at least
  15 seniors (best effort) and reports it in your inbox if it cannot.

## 8. Save and reload

* Use the **Save game** button on the dashboard (it says "Saved!" when done). Save before you quit; do
  not rely on automatic saving.
* Saves are in `~/Library/Application Support/com.apfootballmanager.app/saves`.
* Opening an old save in this build migrates it in place (an older build cannot read it afterwards). Back the
  folder up first: `cp -R "$HOME/Library/Application Support/com.apfootballmanager.app/saves" "$HOME/apfm-saves-backup"`.
* You can save **in the middle of a 2D match**. After *Load Game*, the match should be waiting at the
  instant you saved: press **Continue**, and when the Match Day panel appears choose *Manage the match* again. (The picture before the save point is not
  replayed.)
* Quit completely and reload once during your test: scouting reports, shortlist and the academy must be
  exactly as you left them.

## 9. Reporting a problem

Please send, in one message:

1. What you pressed, step by step, from the main menu (or "after loading save X").
2. What you expected and what happened. A screenshot or a short screen recording helps most.
3. The game's log: `~/Library/Logs/com.apfootballmanager.app/AP Football Manager.log` (the last 100
   lines are enough: `tail -100 "$HOME/Library/Logs/com.apfootballmanager.app/AP Football Manager.log"`).
4. The save: copy the save folder from the path in section 8 if the problem is in a particular career.
5. The terminal output where you ran `npm run tauri dev` if the app crashed.

In-game, *Settings* and the main menu also have a **Report a bug** button.

## 10. Known limits of this build

* An interactive 2D match is **not stored as a replay** after it finishes. Quick simulations and
  "Watch in 2D" matches are.
* Other clubs' matches cannot be watched live in 2D.
* The 2D pitch does not show per-player actions or team phases.
* AI clubs do not run scouting assignments.
* Young players' potential still predicts their development very strongly.
* Some translations (everything except English) have not been reviewed by native speakers.
* The window has been launched and the backend answers, but the full journey has been verified only
  through automated tests; the manual pass in the smoke-test file is the first real run by a person.
