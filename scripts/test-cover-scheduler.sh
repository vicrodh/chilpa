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
    thread)
        if [ "${CHILPA_TSAN_NEGATIVE_CONTROL:-0}" = 1 ]; then
            for control in negative worker-negative; do
                status=0
                QT_QPA_PLATFORM=offscreen CHILPA_TSAN=1 CHILPA_TSAN_CASE=$control \
                    TSAN_OPTIONS="halt_on_error=1:exitcode=66:suppressions=$repo/scripts/tsan-cover-scheduler.supp" \
                    "$temporary/cover_budget" || status=$?
                if [ "$status" -ne 66 ]; then
                    echo "FAIL ThreadSanitizer negative control $control exited $status, expected 66" >&2
                    exit 1
                fi
                echo "PASS ThreadSanitizer negative control $control exited 66"
            done
            exit 0
        fi
        for scheduler_case in core pending running delivery queued; do
            QT_QPA_PLATFORM=offscreen CHILPA_TSAN=1 CHILPA_TSAN_CASE=$scheduler_case \
                TSAN_OPTIONS="halt_on_error=1:suppressions=$repo/scripts/tsan-cover-scheduler.supp" \
                "$temporary/cover_budget"
        done
        ;;
    address) QT_QPA_PLATFORM=offscreen ASAN_OPTIONS=detect_leaks=1:halt_on_error=1 "$temporary/cover_budget" ;;
    none) QT_QPA_PLATFORM=offscreen "$temporary/cover_budget" ;;
esac
