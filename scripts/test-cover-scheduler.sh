#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
temporary=$(mktemp -d)
trap 'rm -r -- "$temporary"' EXIT HUP INT TERM

# shellcheck disable=SC2046
c++ -std=c++17 -O1 -g -fPIC -no-pie -pthread \
    "$repo/crates/chilpa-qt-artwork/tests/cover_budget.cpp" \
    -o "$temporary/cover_budget" \
    $(pkg-config --cflags --libs Qt6Quick Qt6Qml Qt6Network)
"$temporary/cover_budget"
