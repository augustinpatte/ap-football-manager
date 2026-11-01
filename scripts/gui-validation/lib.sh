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
  printf '%s%s\n' "$(cd "$path" && pwd -P)" "$tail"
}
