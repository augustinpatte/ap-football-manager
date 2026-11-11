# GUI validation on an isolated data root

How to run AP Football Manager for a manual GUI check without it reading or writing your real
saves, databases or preferences.

## What is isolated, and how

Setting `APFM_TEST_DATA_ROOT` to an absolute folder switches on a test mode
(`src-tauri/src/test_isolation.rs`). Before Tauri starts, the game points these at that folder:

| Redirected | Why |
|---|---|
| `HOME` | Tauri derives the app data, cache and log folders from it (`~/Library/Application Support/com.apfootballmanager.app`, `~/Library/Caches/...`, `~/Library/Logs/...`) and the importer's default `~/apfootballmanager/data/world.db` lookup |
| `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME` | the same on Linux |
| `TMPDIR` | scratch files |
| `APFM_SAVES_DIR` | pinned to `<root>/saves`, so a value inherited from your shell cannot send saves to your sync folder |

The main window is also made non-persistent (no cookies, local storage or cache survive a launch).

It fails closed:

- a root that is, contains or sits inside your real app data, `~/apfootballmanager`, your
  `APFM_SAVES_DIR` or any of their parents is refused before anything is created (exit 2);
- once Tauri has resolved its app data folder, the game checks it landed inside the root. If the
  platform ignored the redirect, the launch stops **before the first write**.

With the variable unset none of this runs; production behaviour is unchanged.

`world.db` is only ever read. `run-isolated.sh --world-db PATH` copies it into the root and points
`APFM_WORLD_DB` at the copy, so the original cannot be reached at all.

## Run it (macOS)

```bash
git fetch origin && git checkout <branch>
npm ci
```

```bash
scripts/gui-validation/run-isolated.sh --world-db ~/apfootballmanager/data/world.db
```

The script refuses a stub M12 engine, builds a debug binary (`npm run tauri -- build --debug
--no-bundle`), snapshots every real folder the game could touch, launches the game with the isolated
root, and when you close the window checks from the outside that:

1. the game wrote its data under the isolated root (`saves/sync_info.json`);
2. no protected folder changed (fingerprint of every file's path, mtime and size, before and after);
3. your `world.db` is byte-identical (SHA-256).

It exits non-zero and keeps the folder if any check fails. `--no-build` refuses the default
target binary unless it reports the current source revision, so a stale executable cannot bypass
the isolation startup. `--binary PATH` is for a separately verified executable (and controlled
test fixtures). Other options: `--no-install`, `--keep`, `--smoke SECONDS` (launch, wait, close;
for a quick sanity run).

The harness itself is tested without a display or any real data:

```bash
bash scripts/gui-validation/run-isolated.test.sh
```

## What to check by hand

Run it in light and dark, and in at least one non-English language (Settings).

- **Transfers**: list, filters, open a player, make and counter an offer (Esc and a click on the
  backdrop close the counter-offer dialog), empty results.
- **Tactics**: pitch, drag a player, role select on a token, inspector (click the backdrop to close).
- **Calendar**: month and list views, past and future fixtures.
- **Inbox**: open a message with the mouse, with Tab then Enter/Space; bulk-select checkboxes;
  empty inbox; very long subject and sender.
- **Dashboard**: header search (type, Tab into a result, Esc), recent messages, next match.
- **End of season**: awards ceremony, links from a winner to a player and a club.
- **New career**: create manager form (labels focus their field), world select (history depth is
  a radio group: arrow keys move between options), package step (Remove does not toggle the card).
- **Empty / loading / error**: a fresh career before any data, a cancelled load, a failed import.

## Limits

The isolation relies on the OS honouring `HOME` for the folders above. The in-game check
(app data inside the root) and the script's before/after fingerprints exist so that a platform
that does not is reported, not trusted. A launch that never reached the first write leaves
`sync_info.json` absent and the script fails check 1.
