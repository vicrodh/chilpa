#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
temporary=$(mktemp -d)
trap 'rm -r -- "$temporary"' EXIT HUP INT TERM

sanitizer=${CHILPA_SANITIZER:-none}
case "$sanitizer" in
    none) sanitizer_flags= ;;
    thread) sanitizer_flags='-fsanitize=thread -fno-omit-frame-pointer' ;;
    address) sanitizer_flags='-fsanitize=address -fno-omit-frame-pointer' ;;
    *) echo "unknown sanitizer: $sanitizer" >&2; exit 2 ;;
esac

# shellcheck disable=SC2046
c++ -std=c++17 -O1 -g -fPIC -no-pie -pthread \
    $sanitizer_flags \
    "$repo/crates/chilpa-qt-artwork/tests/cover_budget.cpp" \
    -o "$temporary/cover_budget" \
    $(pkg-config --cflags --libs Qt6Quick Qt6Qml Qt6Network)
case "$sanitizer" in
    thread) QT_QPA_PLATFORM=offscreen CHILPA_TSAN=1 TSAN_OPTIONS="halt_on_error=1:suppressions=$repo/scripts/tsan-cover-scheduler.supp" "$temporary/cover_budget" ;;
    address) QT_QPA_PLATFORM=offscreen ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 "$temporary/cover_budget" ;;
    none) QT_QPA_PLATFORM=offscreen "$temporary/cover_budget" ;;
esac
