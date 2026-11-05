#!/usr/bin/env bash
# Shared helpers for the isolated GUI validation. Sourced by run-isolated.sh and its test; written
# for bash 3.2 (the one macOS ships): no associative arrays, no mapfile, no ${var,,}.

APP_IDENTIFIER="com.apfootballmanager.app"

# One line per file: path|mtime|size. Portable across BSD (macOS) and GNU stat.
_stat_lines() {
  if [ "$(uname -s)" = "Darwin" ]; then
    find "$1" -type f -exec stat -f '%N|%m|%z' {} + 2>/dev/null
  else
    find "$1" -type f -exec stat -c '%n|%Y|%s' {} + 2>/dev/null
  fi
}

# A fingerprint of a file or a folder tree: ABSENT when missing, else a hash of every file's
# path, modification time and size. Any created, deleted, resized or touched file changes it.
fingerprint() {
  local target="$1"
  if [ ! -e "$target" ]; then
    echo "ABSENT"
  elif [ -d "$target" ]; then
    { _stat_lines "$target" | LC_ALL=C sort; } | shasum -a 256 | cut -d' ' -f1
  else
    if [ "$(uname -s)" = "Darwin" ]; then
      stat -f '%N|%m|%z' "$target" | shasum -a 256 | cut -d' ' -f1
    else
      stat -c '%n|%Y|%s' "$target" | shasum -a 256 | cut -d' ' -f1
    fi
  fi
}

# Does this binary identify itself as built from the source revision the harness expects? The
# probe returns before Tauri, the crash hook or a display is touched, so a stale default binary is
# rejected before it can write outside a test root.
binary_matches_revision() {
  local binary="$1" expected="$2" reported
  reported="$("$binary" --build-revision 2>/dev/null)" || return 1
  case "$reported" in
    *+"$expected") return 0 ;;
    *) return 1 ;;
  esac
}

# A source or bundled frontend file newer than the default binary means it cannot represent this
# checkout, even if Cargo's build-script cache still has an older revision token.
binary_has_no_newer_inputs() {
  local binary="$1" input
  shift
  for input in "$@"; do
    [ -e "$input" ] || continue
    if [ -f "$input" ]; then
      [ "$input" -nt "$binary" ] && return 1
    elif find "$input" -type f -newer "$binary" -print -quit 2>/dev/null | grep -q .; then
      return 1
    fi
  done
  return 0
}

# The app's build-script output has a directory named `openfootmanager-<hash>`. Do not match the
# repository path: it also contains "openfootmanager" and would accidentally select another
# dependency's newer output.
latest_build_revision() {
  local build_dir="$1" output
  output="$(find "$build_dir" -path '*/openfootmanager-*/output' -type f -exec stat -f '%m|%N' {} \; 2>/dev/null | LC_ALL=C sort -nr | head -n 1 | cut -d'|' -f2-)"
  [ -n "$output" ] || return 1
  sed -n 's/^cargo:rustc-env=OFM_BUILD_REVISION=//p' "$output" | tail -n 1
}

# The places a test session must never touch, one per line, for the real account `$1`.
# `$2` is the player's own APFM_SAVES_DIR, if any.
protected_paths() {
  local home="$1" saves="${2:-}"
  printf '%s\n' \
    "$home/Library/Application Support/$APP_IDENTIFIER" \
    "$home/Library/Caches/$APP_IDENTIFIER" \
    "$home/Library/Logs/$APP_IDENTIFIER" \
    "$home/Library/WebKit/$APP_IDENTIFIER" \
    "$home/Library/Preferences/$APP_IDENTIFIER.plist" \
    "$home/Library/Saved Application State/$APP_IDENTIFIER.savedState" \
    "$home/.local/share/$APP_IDENTIFIER" \
    "$home/.cache/$APP_IDENTIFIER" \
    "$home/.config/$APP_IDENTIFIER" \
    "$home/apfootballmanager"
  if [ -n "$saves" ]; then
    printf '%s\n' "$saves"
  fi
}

# Writes "<fingerprint>  <path>" for each protected path to the file `$1`.
snapshot_protected() {
  local out="$1"; shift
  : > "$out"
  while IFS= read -r path; do
    printf '%s  %s\n' "$(fingerprint "$path")" "$path" >> "$out"
  done < <(protected_paths "$@")
}

# 0 when no protected path changed between two snapshots; otherwise prints the differences.
compare_snapshots() {
  if diff -u "$1" "$2"; then
    return 0
  fi
  return 1
}

# Is `$1` equal to, inside, or an ancestor of any protected path? Resolves symlinks of the part
# that exists. Prints the offending protected path and returns 0 when it overlaps.
overlaps_protected() {
  local candidate resolved
  candidate="$(resolve_path "$1")"
  shift
  while IFS= read -r path; do
    resolved="$(resolve_path "$path")"
    case "$candidate/" in
      "$resolved"/*) echo "$path"; return 0 ;;
    esac
    case "$resolved/" in
      "$candidate"/*) echo "$path"; return 0 ;;
    esac
  done < <(protected_paths "$@")
  return 1
}

# Absolute path with symlinks resolved for the longest existing prefix; the rest is kept as given.
resolve_path() {
  local path="$1" tail=""
  case "$path" in /*) ;; *) path="$PWD/$path" ;; esac
  while [ ! -e "$path" ] && [ "$path" != "/" ]; do
    tail="/$(basename "$path")$tail"
    path="$(dirname "$path")"
  done
  # An existing path that is a file (the app's preferences plist) cannot be `cd`ed into: resolve its
  # folder and keep its name. Left alone, the failed `cd` yielded an empty root that matched every path.
  if [ ! -d "$path" ]; then
    tail="/$(basename "$path")$tail"
    path="$(dirname "$path")"
  fi
  printf '%s%s\n' "$(cd "$path" && pwd -P)" "$tail"
}
