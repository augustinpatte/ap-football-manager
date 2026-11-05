#!/usr/bin/env bash
# F5.2 release gate: do M12 career matches play like football?
#
# Successor to f51-m12-realism-gate.sh, which read two numbers (goals, shots) and so could be
# satisfied by a broken match. This one reads the panel of `examples/f52_realism.rs` in the M12
# checkout: the real career inputs OFM hands M12 (written by the `dump_adapted_career_inputs`
# study from a generated world), played by the preset OFM plays (`f52`), on held-out seeds.
#
#   scripts/f52-m12-realism-gate.sh [world-seed] [inputs] [seeds-per-input]
#
# Every threshold says whether it is SOURCED (a figure with a citation in the repository) or
# PROVISIONAL (an engineering range; the project has no verified tracking or event data for it, see
# apfm-match-lab REFERENCE_DATA_AUDIT.md). A provisional range is a guard against over- and
# under-correction, not a claim about real football.
#
# Needs the M12 checkout (default ~/apfm-match-lab, override with APFM_M12_DIR) at the revision OFM
# pins and a release build of both. Takes several minutes. Writes only under a temp dir.
set -euo pipefail

WORLD_SEED="${1:-7}"
INPUTS="${2:-40}"
SEEDS="${3:-12}"
M12_DIR="${APFM_M12_DIR:-$HOME/apfm-match-lab}"
FLAGS="${F52_FLAGS:-f52}"

DUMP_DIR="$(mktemp -d)"
trap 'rm -rf "$DUMP_DIR"' EXIT

(cd "$(dirname "$0")/../src-tauri" &&
  F44_DUMP_DIR="$DUMP_DIR" F44_SEED="$WORLD_SEED" F44_SAMPLE="$INPUTS" \
    cargo test --release --lib dump_adapted_career_inputs -- --ignored --nocapture >/dev/null)

PANEL="$(cd "$M12_DIR" && cargo build --release --example f52_realism 2>/dev/null &&
  F52_DETAIL=1 ./target/release/examples/f52_realism "$DUMP_DIR" "$SEEDS" "prod|-|$FLAGS")"
echo "$PANEL"

# field NAME -> the number after it on the first (summary) line
field() { sed -n 1p <<<"$PANEL" | sed -E "s/.* $1 ([0-9.]+).*/\\1/"; }
GOALS="$(field goals)"
SHOTS="$(field shots)"
SOT="$(sed -n 1p <<<"$PANEL" | sed -E 's/.* sot [0-9.]+ \(([0-9]+)%\).*/\1/')"
RED="$(field red)"
PEN="$(field pen)"
LIVE_DEAD="$(sed -n 's/.* dead \([0-9]*\)%.*/\1/p' <<<"$PANEL" | head -1)"
# mean km per 90 of the outfield positions, from the detail line
KM="$(sed -n 's/^    km\/90 by position://p' <<<"$PANEL" | tr ' ' '\n' | grep -E '^[0-9]+\.[0-9]$' | paste -sd' ' -)"
OUTFIELD_KM="$(sed -n 's/^    km\/90 by position://p' <<<"$PANEL" | sed -E 's/GK [0-9.]+ \(sprints [0-9]+\)//' |
  grep -oE ' [0-9]+\.[0-9] \(' | tr -d ' (' | awk '{ s += $1; n++ } END { if (n) printf "%.1f", s / n }')"

fail=0
check() { # name value min max provenance
  if awk -v v="$2" -v lo="$3" -v hi="$4" 'BEGIN { exit !(v >= lo && v <= hi) }'; then
    printf 'PASS  %-34s %8s  in [%s, %s]  (%s)\n' "$1" "$2" "$3" "$4" "$5"
  else
    printf 'FAIL  %-34s %8s  not in [%s, %s]  (%s)\n' "$1" "$2" "$3" "$4" "$5" >&2
    fail=1
  fi
}

# Goals: ceiling is the mission's stated tolerance (football is about 2.6, the project's reference).
check "goals a match"                 "$GOALS" 2.0 3.3  "ceiling: mission tolerance; floor provisional"
check "shots a match"                 "$SHOTS" 20  32   "ceiling: mission tolerance; floor provisional"
check "shots on target (% of shots)"  "$SOT"   28  55   "PROVISIONAL"
check "dismissals a match"            "$RED"   0.03 0.16 "ceiling SOURCED (Premier League 2023/24, F4.2 report)"
check "penalties a match"             "$PEN"   0.15 0.45 "PROVISIONAL (about 0.27 in the Premier League, unverified here)"
check "ball dead (% of match)"        "$LIVE_DEAD" 22 45 "PROVISIONAL (effective playing time 55 to 65 min is common knowledge, unsourced)"
check "outfield km per 90 (mean)"     "${OUTFIELD_KM:-0}" 8 14 "PROVISIONAL (no tracking source)"

[ "$fail" -eq 0 ] && echo "PASS: every F5.2 realism check holds (distances: $KM)"
exit "$fail"
