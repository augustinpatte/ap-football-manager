#!/usr/bin/env bash
# Tests for the isolated GUI validation harness, using a fake "game" so no display, Tauri build or
# real player data is involved. Run: bash scripts/gui-validation/run-isolated.test.sh
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
. "$here/lib.sh"
failures=0
pass() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; failures=$((failures + 1)); }
check() { local name="$1"; shift; if "$@" >/dev/null 2>&1; then pass "$name"; else fail "$name"; fi; }
check_not() { local name="$1"; shift; if "$@" >/dev/null 2>&1; then fail "$name"; else pass "$name"; fi; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fake_home="$work/home"
mkdir -p "$fake_home/apfootballmanager/data" "$fake_home/Library/Application Support/$APP_IDENTIFIER/saves"
echo "world" > "$fake_home/apfootballmanager/data/world.db"
echo "save" > "$fake_home/Library/Application Support/$APP_IDENTIFIER/saves/career.db"

# Given a protected tree / When a file is touched, added, removed / Then the fingerprint changes.
f0="$(fingerprint "$fake_home/apfootballmanager")"
[ "$f0" = "$(fingerprint "$fake_home/apfootballmanager")" ] && pass "fingerprint is stable" || fail "fingerprint is stable"
echo "more" >> "$fake_home/apfootballmanager/data/world.db"
[ "$f0" != "$(fingerprint "$fake_home/apfootballmanager")" ] && pass "fingerprint sees a modification" || fail "fingerprint sees a modification"
f1="$(fingerprint "$fake_home/apfootballmanager")"
touch "$fake_home/apfootballmanager/data/new.txt"
[ "$f1" != "$(fingerprint "$fake_home/apfootballmanager")" ] && pass "fingerprint sees a new file" || fail "fingerprint sees a new file"
[ "$(fingerprint "$work/missing")" = "ABSENT" ] && pass "a missing path is ABSENT" || fail "a missing path is ABSENT"

# Given a candidate root / When it equals, sits inside or contains a protected path / Then refused.
check "refuses the real app data folder" overlaps_protected "$fake_home/Library/Application Support/$APP_IDENTIFIER" "$fake_home"
check "refuses a folder inside real app data" overlaps_protected "$fake_home/Library/Application Support/$APP_IDENTIFIER/x" "$fake_home"
check "refuses the home folder (it contains protected data)" overlaps_protected "$fake_home" "$fake_home"
check "refuses a custom saves folder" overlaps_protected "$work/sync/inside" "$fake_home" "$work/sync"
check_not "accepts an unrelated temp folder" overlaps_protected "$work/sandbox" "$fake_home"
ln -s "$fake_home/apfootballmanager" "$work/innocent"
check "refuses a symlink into protected data" overlaps_protected "$work/innocent" "$fake_home"

# End to end with a fake game. It writes where APFM_TEST_DATA_ROOT says, like the real one.
good="$work/good-game"
cat > "$good" <<'GAME'
#!/usr/bin/env bash
dir="$APFM_TEST_DATA_ROOT/home/.local/share/com.apfootballmanager.app/saves"
mkdir -p "$dir" && echo '{}' > "$dir/sync_info.json"
GAME
leaky="$work/leaky-game"
cat > "$leaky" <<'GAME'
#!/usr/bin/env bash
dir="$APFM_TEST_DATA_ROOT/home/.local/share/com.apfootballmanager.app/saves"
mkdir -p "$dir" && echo '{}' > "$dir/sync_info.json"
echo "oops" >> "$REAL_HOME_FOR_TEST/apfootballmanager/data/world.db"
GAME
silent="$work/silent-game"
printf '#!/usr/bin/env bash\nexit 0\n' > "$silent"
chmod +x "$good" "$leaky" "$silent"
export REAL_HOME_FOR_TEST="$fake_home"

check "a well-behaved game passes" env HOME="$fake_home" "$here/run-isolated.sh" --binary "$good" --smoke 0
check_not "a game that writes real data is caught" env HOME="$fake_home" "$here/run-isolated.sh" --binary "$leaky" --smoke 0
check_not "a game that never writes into the root is caught" env HOME="$fake_home" "$here/run-isolated.sh" --binary "$silent" --smoke 0
cp "$fake_home/apfootballmanager/data/world.db" "$work/world.db"
# A game that rewrites the original world.db (outside any protected folder) is caught by the hash alone.
cat > "$work/db-game" <<'GAME'
#!/usr/bin/env bash
dir="$APFM_TEST_DATA_ROOT/home/.local/share/com.apfootballmanager.app/saves"
mkdir -p "$dir" && echo '{}' > "$dir/sync_info.json"
echo "tampered" >> "$ORIGINAL_WORLD_DB"
GAME
chmod +x "$work/db-game"
export ORIGINAL_WORLD_DB="$work/world.db"
check_not "a changed world.db is caught" env HOME="$fake_home" "$here/run-isolated.sh" --binary "$work/db-game" --world-db "$work/world.db"
cp "$work/world.db" "$work/world.copy.db"
check "an untouched world.db passes" env HOME="$fake_home" "$here/run-isolated.sh" --binary "$good" --world-db "$work/world.copy.db"

[ "$failures" -eq 0 ] && { echo "all passed"; exit 0; } || { echo "$failures failed"; exit 1; }
