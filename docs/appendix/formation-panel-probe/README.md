# Formation & Substitutions panel: browser measurement (ALPHA6.2)

Measures, in real Google Chrome (headless), the **real** `InteractiveMatchScreen` with its Formation &
Substitutions tab open: the actual `FormationSubstitutionsPanel`, `FormationPitch`, `Tabs`, the screen's
own list controls and the real stylesheet. Only the Tauri IPC is replaced (`mockIPC`), and what it
answers is **the engine's own views**: one real M12 session per supported formation, produced by
`cargo test --lib write_formation_probe -- --ignored` (it starts a real match, sends the real
`Tactics` order for the formation, advances 40 s and reads the real `InteractiveView`). The
"busy" variant is that real view with its data changed the way a match changes it: the last outfield
player moved to `sent_off`, one substitution queued, two players moved to `used` (one injured), one
player on the pitch injured. The "long" variant gives every player a 45-character surname.

Re-run: copy `main.tsx.txt` to `src/dev/_formationProbe/main.tsx` and `formation-probe.html.txt` to
`formation-probe.html` at the repository root, generate `src/dev/_formationProbe/formations.json` with
the command above (`APFM_FORMATION_PROBE_OUT=<path>`), run `npx vite --port 5199`, then
`run_probe.sh.txt <window width> <formation> <full|busy> <0|1>` (it prints the probe's JSON). Delete the
copied files afterwards; they are a measurement aid, not product code.

`results.json`: 72 runs = window widths 960, 1280, 1440, 1728 x 6 formations (4-4-2, 4-3-3, 4-2-3-1,
3-5-2, 5-3-2, 4-5-1) x {normal, busy (ten players), busy with long names}. Per run: tokens on the pitch,
pairs of token footprints that overlap, tokens outside the pitch, controls whose centre is covered by
another element or lies outside the viewport, whether a keyboard-focused token has a visible ring,
horizontal document overflow, and the name spans that are cut with an ellipsis.

The viewport is 15 px narrower than the window (a scrollbar), so 1280 is measured at 1265, and so on.
The pitch is 416 x 624 px at every width. The two PNGs are the panel at the end of the run for a
ten-man 3-5-2 at 1280 and a ten-man 4-5-1 at 960, with a substitution queued, a player sent off, one
substituted and one injured (the yellow ring on the goalkeeper is the probe's keyboard-focus test).

Not measured here: a real Tab-key walk (headless dump-dom cannot send keys; Enter/Space and focus
order are covered in jsdom), and the macOS Tauri webview.
