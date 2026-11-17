# Sourced by the visual tasks. Needs common.sh first.
# shellcheck shell=bash

# The visual harness's Python side: python3 with numpy and Pillow.
need_python() {
  command -v python3 >/dev/null 2>&1 || die "python3 not found on PATH: install Python 3"
  python3 -c 'import numpy, PIL' >/dev/null 2>&1 ||
    die "python3 cannot import numpy and PIL: install them (python-numpy python-pillow on Arch, python3-numpy python3-pil on Debian/Ubuntu, or pip install numpy pillow)"
}
