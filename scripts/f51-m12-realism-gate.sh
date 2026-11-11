#!/usr/bin/env bash
# F5.1 release-blocker gate: do M12 career matches score like football?
#
# Reproduces the measurement behind docs/APFM_RUN_F5_1_INTELLIGENT_WORLD.md section 14: the real
# career inputs OFM hands M12 (written by the `dump_adapted_career_inputs` study from a generated
# world), played by the production engine preset (`f44`) on held-out seeds, against the ceiling a
# football reference allows. It FAILS today, on purpose: that failure is the blocker, and the
# script is what turns "fixed" into something a build can check.
#
#   scripts/f51-m12-realism-gate.sh [world-seed] [inputs] [seeds-per-input]
#
# Needs the M12 checkout (default ~/apfm-match-lab, override with APFM_M12_DIR) at the revision
# OFM pins, and a release build of both. Takes a few minutes. Writes only under a temp dir.
set -euo pipefail

WORLD_SEED="${1:-7}"
INPUTS="${2:-40}"
SEEDS="${3:-12}"
M12_DIR="${APFM_M12_DIR:-$HOME/apfm-match-lab}"
# Football's reference is about 2.6 goals a match (Premier League 2023/24: 2.8); the statistical
# tier of this game is about 2.5. A tolerance for scoring variance and the engine's open-play
# convention is built into the ceiling: matches above it are not "a bit high", they are 1.5x football.
GOALS_CEILING="${GOALS_CEILING:-3.3}"
SHOTS_CEILING="${SHOTS_CEILING:-32}"

DUMP_DIR="$(mktemp -d)"
trap 'rm -rf "$DUMP_DIR"' EXIT

(cd "$(dirname "$0")/../src-tauri" &&
  F44_DUMP_DIR="$DUMP_DIR" F44_SEED="$WORLD_SEED" F44_SAMPLE="$INPUTS" \
    cargo test --release --lib dump_adapted_career_inputs -- --ignored --nocapture >/dev/null)

REPORT="$(cd "$M12_DIR" &&
  F44_FLAGS="f44" cargo run --release --example f44_career_inputs -- "$DUMP_DIR" "$SEEDS" base 2>/dev/null |
  grep '^base')"
echo "$REPORT"

GOALS="$(sed -E 's/.*goals ([0-9.]+).*/\1/' <<<"$REPORT")"
SHOTS="$(sed -E 's/.*shots ([0-9.]+).*/\1/' <<<"$REPORT")"

fail=0
if awk -v g="$GOALS" -v c="$GOALS_CEILING" 'BEGIN { exit !(g > c) }'; then
  echo "FAIL: $GOALS goals a match, ceiling $GOALS_CEILING" >&2
  fail=1
fi
if awk -v s="$SHOTS" -v c="$SHOTS_CEILING" 'BEGIN { exit !(s > c) }'; then
  echo "FAIL: $SHOTS shots a match, ceiling $SHOTS_CEILING" >&2
  fail=1
fi
[ "$fail" -eq 0 ] && echo "PASS: $GOALS goals and $SHOTS shots a match are within the ceilings"
exit "$fail"
