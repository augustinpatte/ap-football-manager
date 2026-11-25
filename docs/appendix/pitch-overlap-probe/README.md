# Pitch token overlap probe (ALPHA6 step 8)

Measures, in a real browser, whether the player tokens on `FormationPitch` overlap.

To re-run: copy `main.tsx.txt` to `src/dev/_pitchProbe/main.tsx` and `pitch-probe.html.txt` to
`pitch-probe.html` at the repository root, run `npx vite --port 5199`, then

```
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --window-size=1700,1100 \
  --virtual-time-budget=15000 --dump-dom http://localhost:5199/pitch-probe.html
```

and read the JSON in `<pre id="probe-result">`. Delete both copied files afterwards: they are a
measurement aid, not product code. Transitions are disabled in the page (headless virtual time does
not advance CSS transitions, so the scale would otherwise read as not yet applied).

`pitch_probe_after.png`: left of each pair is the old 96 px card with its role select, right is the
new compact token, at pitch widths 300, 360 and 520 px.
