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

# `qt_version_of <tool>`: the Qt version the Qt tool at path <tool> belongs
# to, or nothing. qmllint and qml answer --version themselves; qmltestrunner
# does not, so the Qt tools beside it are asked (Qt's installers ship them
# unsuffixed). Qt 5's tools answer too, so only an answer of 6 counts.
qt_version_of() {
  local tool="$1" real link dir ask out
  # QMLTESTRUNNER may be a bare name; its Qt is beside the one PATH finds.
  case "$tool" in
    */*) real="$tool" ;;
    *) real="$(command -v "$tool" || printf '%s' "$tool")" ;;
  esac
  # A link on PATH (~/bin/qmltestrunner6 -> ~/Qt/6.12.0/gcc_64/bin/...) has
  # no Qt beside it; the file it names does. readlink -f is not on every
  # macOS, so each hop is followed by hand.
  while [ -L "$real" ]; do
    link="$(readlink "$real")"
    case "$link" in
      /*) real="$link" ;;
      *) real="$(dirname "$real")/$link" ;;
    esac
  done
  dir="$(dirname "$real")"
  case "${tool##*/}" in
    qmllint*) out="$("$tool" --version 2>/dev/null || true)" ;;
    # qml makes its GUI application before it reads --version, so with no
    # display it would answer nothing without the offscreen platform.
    qml | qml6) out="$(QT_QPA_PLATFORM=offscreen "$tool" --version 2>/dev/null || true)" ;;
    *) out="" ;;
  esac
  if [[ "$out" =~ (^|[^0-9])(6\.[0-9]+\.[0-9]+) ]]; then
    printf '%s\n' "${BASH_REMATCH[2]}"
    return
  fi
  # qtpaths and qmake come with Qt's base development tools, which a
  # distribution's declarative tools do not pull in; the qmllint and qml
  # installed beside the runner are of its Qt too.
  for ask in "qtpaths6 --qt-version" "qmake6 -query QT_VERSION" "qtpaths --qt-version" "qmake -query QT_VERSION" \
    "qmllint6 --version" "qmllint --version" "qml6 --version" "qml --version"; do
    [ -x "$dir/${ask%% *}" ] || continue
    # shellcheck disable=SC2086 # the flags are a word list
    out="$(QT_QPA_PLATFORM=offscreen "$dir/${ask%% *}" ${ask#* } 2>/dev/null || true)"
    if [[ "$out" =~ (^|[^0-9])(6\.[0-9]+\.[0-9]+) ]]; then
      printf '%s\n' "${BASH_REMATCH[2]}"
      return
    fi
  done
}

# `qt_pin_mismatches`: a line for each Qt tool the tasks run (found as they
# find it) that belongs to another Qt than the pinned minor, with what to set
# so the tasks take a pinned one instead. A tool whose Qt cannot be told is
# named too, since it may be any Qt. Nothing when all are pinned, or for a
# tool that is not there.
qt_pin_mismatches() {
  local name path version use fix
  for name in qmltestrunner qmllint qml6; do
    case "$name" in
      qml6)
        path="$(command -v qml6 || true)"
        use="mise run qml:check, dev and visual run"
        fix="put Qt $qt_pinned's qml first on PATH as qml6"
        ;;
      qmltestrunner)
        path="$(qt6_tool qmltestrunner)"
        use="mise run test's QML end-to-end tests run"
        fix="set QMLTESTRUNNER to its qmltestrunner"
        ;;
      qmllint)
        path="$(qt6_tool qmllint)"
        use="mise run qml:check runs"
        fix="set QMLLINT to its qmllint"
        ;;
    esac
    [ -n "$path" ] || continue
    version="$(qt_version_of "$path")"
    if [ -z "$version" ]; then
      echo "$name ($path) answers no Qt 6 version, and no qtpaths or qmake beside it does, so $use it on a Qt that may not be the pinned $qt_pinned: install Qt $qt_pinned (Qt's online installer, or aqt install-qt) and $fix"
      continue
    fi
    case "$version" in "$qt_pinned".*) continue ;; esac
    echo "$name ($path) is Qt $version, not the pinned Qt $qt_pinned, and $use it: install Qt $qt_pinned (Qt's online installer, or aqt install-qt) and $fix"
  done
}
