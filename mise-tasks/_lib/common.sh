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

# Cargo's target directory as cargo resolves it: CARGO_TARGET_DIR, a
# machine-local .cargo/config.toml, or <root>/target.
target_dir() {
  cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml" |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
}

# `build_eldur [cargo flags]`: builds the eldur binary and prints its path, as
# cargo reports it: a build target (CARGO_BUILD_TARGET) or profile moves it.
build_eldur() {
  local exe
  exe="$(cargo build -q --locked --manifest-path "$root/Cargo.toml" -p eldur_cli --bin eldur \
    --message-format=json-render-diagnostics "$@" |
    sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -n 1)"
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
