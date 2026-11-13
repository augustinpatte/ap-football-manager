#!/usr/bin/env bash
# Launches AP Football Manager for a manual GUI validation with every persistent write redirected
# into a throw-away folder, then proves nothing outside it changed.
#
#   scripts/gui-validation/run-isolated.sh [--world-db PATH] [--binary PATH] [--no-build]
#                                          [--no-install] [--keep] [--smoke SECONDS]
#
# How the isolation works: the app reads APFM_TEST_DATA_ROOT (src-tauri/src/test_isolation.rs) and,
# before Tauri starts, points HOME, XDG_*, TMPDIR and APFM_SAVES_DIR at it and makes the webview
# non-persistent. It refuses to start if that folder overlaps real player data, and again if
# Tauri's app data folder lands outside it. This script adds an independent check from outside:
# fingerprints of the real folders before and after, and a hash of your world.db.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root_dir="$(cd "$here/../.." && pwd)"
# shellcheck source=lib.sh
. "$here/lib.sh"

world_db=""; binary=""; binary_was_provided=0; build=1; install=1; keep=0; smoke=0
while [ $# -gt 0 ]; do
  case "$1" in
    --world-db) world_db="${2:?--world-db needs a path}"; shift 2 ;;
    --binary) binary="${2:?--binary needs a path}"; binary_was_provided=1; build=0; shift 2 ;;
    --no-build) build=0; shift ;;
    --no-install) install=0; shift ;;
    --keep) keep=1; shift ;;
    --smoke) smoke="${2:?--smoke needs seconds}"; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 64 ;;
  esac
done

real_home="${HOME:?HOME is not set}"
real_saves="${APFM_SAVES_DIR:-}"

# --- 1+2. Build (only when this script builds): the real M12 engine, never a stub ---------------
if [ "$build" -eq 1 ]; then
  engine="$(cd "$root_dir/src-tauri/crates/ofm_core/../../../../../" && pwd)/apfm-match-lab"
  if [ ! -f "$engine/Cargo.toml" ]; then
    echo "M12 engine not found at $engine (see scripts/apfm/run-m12-tests.sh)." >&2; exit 2
  fi
  if grep -rqs "M12 stub" "$engine/src"; then
    echo "$engine is a stub, not the real engine; refusing to validate the GUI against it." >&2
    exit 3
  fi
  cd "$root_dir"
  if [ "$install" -eq 1 ]; then npm ci; fi
  npm run tauri -- build --debug --no-bundle
fi
[ -n "$binary" ] || binary="$root_dir/src-tauri/target/debug/openfootmanager"
[ -x "$binary" ] || { echo "binary not found or not executable: $binary" >&2; exit 2; }

# `--no-build` otherwise picks the default target binary. It must be the binary Cargo most
# recently built from these sources: an old target lacks APFM_TEST_DATA_ROOT and could touch the
# account before the harness gets a chance to compare fingerprints. An explicit --binary is kept
# for controlled fixtures and for a binary an operator has separately verified.
if [ "$build" -eq 0 ] && [ "$binary_was_provided" -eq 0 ]; then
  expected_revision="$(latest_build_revision "$root_dir/src-tauri/target/debug/build" || true)"
  build_inputs=(
    "$root_dir/dist"
    "$root_dir/src-tauri/src"
    "$root_dir/src-tauri/crates"
    "$root_dir/src-tauri/capabilities"
    "$root_dir/src-tauri/assets"
    "$root_dir/src-tauri/data"
    "$root_dir/src-tauri/databases"
    "$root_dir/src-tauri/Cargo.toml"
    "$root_dir/src-tauri/Cargo.lock"
    "$root_dir/src-tauri/tauri.conf.json"
    "$root_dir/src-tauri/build.rs"
    "$root_dir/src-tauri/build_revision.rs"
  )
  if [ -z "$expected_revision" ] || ! binary_matches_revision "$binary" "$expected_revision" || ! binary_has_no_newer_inputs "$binary" "${build_inputs[@]}"; then
    echo "refusing --no-build: the default binary is not built from the current source revision; run without --no-build or pass an explicitly verified --binary." >&2
    exit 3
  fi
fi

# --- 3. The throw-away root, refused if it could touch real data ------------------------------
sandbox="$(mktemp -d "${TMPDIR:-/tmp}/apfm-gui-test.XXXXXX")"
if hit="$(overlaps_protected "$sandbox" "$real_home" "$real_saves")"; then
  echo "the temporary folder $sandbox overlaps protected data ($hit)." >&2; exit 5
fi
echo "isolated root: $sandbox"

# A copy of world.db, so even a bug cannot reach the original.
world_before=""
if [ -n "$world_db" ]; then
  [ -f "$world_db" ] || { echo "world.db not found: $world_db" >&2; exit 2; }
  world_before="$(shasum -a 256 "$world_db" | cut -d' ' -f1)"
  mkdir -p "$sandbox/fixtures"
  cp "$world_db" "$sandbox/fixtures/world.db"
  export APFM_WORLD_DB="$sandbox/fixtures/world.db"
else
  unset APFM_WORLD_DB || true
fi

# --- 4. Before ---------------------------------------------------------------------------------
before="$sandbox/protected.before"; after="$sandbox/protected.after"
snapshot_protected "$before" "$real_home" "$real_saves"

# --- 5. Launch ---------------------------------------------------------------------------------
export APFM_TEST_DATA_ROOT="$sandbox"
status=0
if [ "$smoke" -gt 0 ]; then
  "$binary" > "$sandbox/app.stdout.log" 2>&1 &
  pid=$!
  sleep "$smoke"
  if kill -0 "$pid" 2>/dev/null; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true
  else wait "$pid" || status=$?; fi
else
  echo "Close the game window when you are done."
  "$binary" || status=$?
fi
unset APFM_TEST_DATA_ROOT

# --- 6. After: prove nothing real was touched --------------------------------------------------
fail=0
canary="$(find "$sandbox" -name sync_info.json -path '*saves*' 2>/dev/null | head -n 1)"
if [ -z "$canary" ]; then
  echo "FAIL: the game never wrote into the isolated root (no saves/sync_info.json)." >&2; fail=1
else
  echo "ok: the game wrote its data into the isolated root ($canary)"
fi
snapshot_protected "$after" "$real_home" "$real_saves"
if compare_snapshots "$before" "$after"; then
  echo "ok: no protected folder changed"
else
  echo "FAIL: a protected folder changed (diff above)." >&2; fail=1
fi
if [ -n "$world_db" ]; then
  if [ "$(shasum -a 256 "$world_db" | cut -d' ' -f1)" = "$world_before" ]; then
    echo "ok: world.db is byte-identical"
  else
    echo "FAIL: world.db changed." >&2; fail=1
  fi
fi

if [ "$keep" -eq 1 ] || [ "$fail" -ne 0 ]; then
  echo "kept for inspection: $sandbox"
else
  rm -rf "$sandbox"
fi
[ "$fail" -eq 0 ] || exit 1
[ "$status" -eq 0 ] || { echo "the game exited with status $status" >&2; exit "$status"; }
echo "GUI session finished with no write outside the isolated root."
