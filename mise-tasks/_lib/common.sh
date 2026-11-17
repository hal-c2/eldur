# Sourced by the tasks; not a task itself (mise runs only executable files).
# shellcheck shell=bash

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# `die <what is missing and how to get it>`: one line, then a non-zero exit.
die() {
  echo "${task_name:-mise-tasks}: $*" >&2
  exit 1
}

# mise runs every task from the project root, but a path the caller typed is
# relative to where they ran mise.
from_caller() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "${MISE_ORIGINAL_CWD:-$PWD}" "$1" ;;
  esac
}

# `json_string <key>`: the string <key> on the last stdin line that has it,
# decoded. Cargo writes JSON on one line per message; a path can hold a quote
# or a backslash, which it escapes. The other escapes are control characters,
# which no path here holds, so they fail instead of being guessed at.
json_string() {
  local raw bs='\' q='"' slash='/' mark=$'\001'
  raw="$(sed -nE 's/.*"'"$1"'":"(([^"\\]|\\.)*)".*/\1/p' | tail -n 1)"
  raw="${raw//"$bs$bs"/$mark}"
  raw="${raw//"$bs$q"/$q}"
  raw="${raw//"$bs$slash"/$slash}"
  case "$raw" in *"$bs"*) die "cargo's $1 has an escape this script cannot decode: $raw" ;; esac
  printf '%s\n' "${raw//$mark/$bs}"
}

# Cargo's target directory as cargo resolves it: CARGO_TARGET_DIR, a
# machine-local .cargo/config.toml, or <root>/target.
target_dir() {
  local meta dir
  meta="$(cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml")" ||
    die "cargo metadata failed, so the target directory is unknown"
  dir="$(printf '%s\n' "$meta" | json_string target_directory)"
  [[ -n "$dir" ]] || die "cargo metadata named no target directory"
  printf '%s\n' "$dir"
}

# `build_eldur [cargo flags]`: builds the eldur binary and prints its path, as
# cargo reports it: a build target (CARGO_BUILD_TARGET) or profile moves it.
# Callers run it in $(...), where bash drops `set -e`, so cargo's failure is
# checked here.
build_eldur() {
  local out exe
  out="$(cargo build -q --locked --manifest-path "$root/Cargo.toml" -p eldur_cli --bin eldur \
    --message-format=json-render-diagnostics "$@")" || die "cargo could not build eldur_cli"
  exe="$(printf '%s\n' "$out" | json_string executable)"
  [[ -n "$exe" ]] || die "cargo built eldur_cli but named no eldur executable"
  printf '%s\n' "$exe"
}

# `qt6_tool <name>`: the path of Qt 6's <name> (qmllint, qmltestrunner), or
# nothing. Arch's /usr/bin/qmllint and /usr/bin/qmltestrunner are Qt 5's, so
# the plain name on PATH is the last resort, taken only when it is known to be
# Qt 6's.
# An environment variable named after the tool (QMLLINT, QMLTESTRUNNER) wins.
qt6_tool() {
  local name="$1" var dir bins
  var="$(printf '%s' "$name" | tr '[:lower:]' '[:upper:]')"
  if [ -n "${!var:-}" ]; then
    printf '%s\n' "${!var}"
    return
  fi
  if command -v "${name}6" >/dev/null 2>&1; then
    command -v "${name}6"
    return
  fi
  for bins in "$(qtpaths6 --query QT_INSTALL_BINS 2>/dev/null)" \
    "$(qmake6 -query QT_INSTALL_BINS 2>/dev/null)" \
    /usr/lib/qt6/bin /usr/lib64/qt6/bin /usr/libexec/qt6 \
    /opt/homebrew/opt/qt/bin /usr/local/opt/qt/bin; do
    dir="${bins%/}"
    if [ -n "$dir" ] && [ -x "$dir/$name" ]; then
      printf '%s\n' "$dir/$name"
      return
    fi
  done
  # Qt 5's qtpaths has no --query, but both answer --qt-version. With no
  # answer the tool may be Qt 5's, so it is not taken.
  if command -v "$name" >/dev/null 2>&1; then
    case "$(qtpaths --qt-version 2>/dev/null)" in
      6.*) command -v "$name" ;;
    esac
  fi
}

# How to get Qt 6's QML tools, for the messages that say one is missing.
# shellcheck disable=SC2034 # read by the tasks that source this file
qt6_install_hint="install Qt 6's QML tools: qt6-declarative (Arch), qt6-declarative-dev-tools and qml6-module-qtquick (Debian/Ubuntu), or qt (Homebrew)"

# The Qt minor that the QML end-to-end tests, qml:check and docs/spec/ measure
# (AGENTS.md, docs/ARCHITECTURE.md). Another minor lays out, lints and runs
# JavaScript differently, so a pass or failure there says nothing about 6.11.
qt_pinned="6.11"

# `qt6_version`: "<version> <tool>" for the Qt 6 on PATH, or nothing. Qt 5's
# unsuffixed qtpaths and qmake answer too, so theirs counts only when it is 6.
qt6_version() {
  local ask tool out
  for ask in "qtpaths6 --qt-version" "qmake6 -query QT_VERSION" "qml6 --version" \
    "qtpaths --qt-version" "qmake -query QT_VERSION"; do
    tool="${ask%% *}"
    command -v "$tool" >/dev/null 2>&1 || continue
    # shellcheck disable=SC2086 # the tool and its flags are a word list
    out="$($ask 2>/dev/null)" || continue
    [[ "$out" =~ ([0-9]+\.[0-9]+\.[0-9]+) ]] || continue
    case "${BASH_REMATCH[1]}" in
      6.*)
        printf '%s %s\n' "${BASH_REMATCH[1]}" "$tool"
        return
        ;;
    esac
  done
}

# `qt_pin_mismatch`: one line saying the Qt 6 found is not the pinned minor
# and how to fix that, or nothing when it is (or when there is no Qt 6).
qt_pin_mismatch() {
  local found version tool
  found="$(qt6_version)"
  [ -n "$found" ] || return 0
  version="${found%% *}"
  tool="${found#* }"
  case "$version" in "$qt_pinned".*) return 0 ;; esac
  echo "Qt $version ($(command -v "$tool")) is not the pinned Qt $qt_pinned, so the QML end-to-end tests and qml:check measure another Qt; install Qt $qt_pinned (Qt's online installer, or aqt install-qt) and put its bin directory first on PATH"
}
