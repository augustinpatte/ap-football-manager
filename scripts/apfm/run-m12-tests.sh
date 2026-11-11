#!/usr/bin/env bash
# Runs the tests that need the REAL M12 engine (`apfm-match-lab`), and refuses to report success
# when it is absent. `ofm_core` depends on it by path (src-tauri/crates/ofm_core/Cargo.toml):
#   ../../../../../apfm-match-lab, resolved from src-tauri/crates/ofm_core.
# With the repository at ~/apfm-migration/openfootmanager that is ~/apfm-match-lab. For another
# layout, point APFM_MATCH_LAB at the engine checkout and this script symlinks the expected path.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
# macOS `realpath` has no GNU `-m` flag.  The parent directory always exists in
# the supported checkout layout, so resolving it with `cd` is both portable and
# gives the canonical engine path before we test for its manifest.
expected="$(cd "$root/src-tauri/crates/ofm_core/../../../../../" && pwd)/apfm-match-lab"

if [ ! -f "$expected/Cargo.toml" ]; then
  if [ -n "${APFM_MATCH_LAB:-}" ] && [ -f "$APFM_MATCH_LAB/Cargo.toml" ]; then
    echo "linking $expected -> $APFM_MATCH_LAB"
    ln -s "$APFM_MATCH_LAB" "$expected"
  else
    echo "M12 engine not found at $expected." >&2
    echo "Check out apfm-match-lab there, or set APFM_MATCH_LAB=<checkout> and re-run." >&2
    exit 2
  fi
fi

if grep -rqs "M12 stub" "$expected/src"; then
  echo "$expected is a stub, not the real engine; refusing to report a result." >&2
  exit 3
fi

cd "$root/src-tauri"
# Each filter must match at least one test: a zero-test selection is not a pass.
run() {
  local label="$1"; shift
  local out
  out="$("$@" 2>&1)" || { echo "$out"; echo "FAILED: $label" >&2; exit 1; }
  echo "$out" | tail -n 3
  if echo "$out" | grep -q "running 0 tests"; then
    echo "NO TESTS MATCHED: $label" >&2
    exit 4
  fi
}
run "ofm_core spatial_runner" cargo test -p ofm_core --lib spatial_runner
run "ofm_core spatial_* (adapter, projection, output)" cargo test -p ofm_core --lib spatial_
run "openfootmanager commands::spatial_match" cargo test -p openfootmanager --lib commands::spatial_match
echo "M12-dependent tests passed against $expected"
