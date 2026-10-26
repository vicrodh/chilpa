#include "../cxx/storage_events.h"
#include <QtGui/QGuiApplication>
#include <QtCore/QDir>
#include <QtTest/QSignalSpy>
#include <QtTest/QTest>
#include <cstdio>

#if defined(Q_OS_LINUX) || defined(Q_OS_ANDROID)
// I use a quiet file for teardown tests; production supplies its endpoint from chilpa-dirs.
extern "C" const char *chilpa_mount_events_file() { return "/dev/null"; }
#endif

int main(int argc, char **argv) {
    QGuiApplication application(argc, argv);
    StorageEvents events;
    QSignalSpy changed(&events, &StorageEvents::changed);
    if (events.active()) return 1;
    events.setActive(true);
    events.setActive(false);
    events.setActive(true);
    if (!changed.wait(1500)) return 6;
    events.setActive(false);
    changed.clear();
#if defined(Q_OS_LINUX)
    const auto descriptors = [] { return QDir("/proc/self/fd").entryList(QDir::AllEntries | QDir::NoDotAndDotDot).size(); };
    const auto before = descriptors();
#endif
    for (int cycle = 0; cycle < 100; ++cycle) {
        events.setActive(true);
        if (!events.active()) return 2;
        events.setActive(false);
        if (events.active()) return 3;
    }
    QTest::qWait(900);
    if (changed.count() != 0) return 4;
#if defined(Q_OS_LINUX)
    const auto after = descriptors();
    if (before != after) { std::fprintf(stderr, "descriptor count changed: %lld -> %lld\n", static_cast<long long>(before), static_cast<long long>(after)); return 5; }
    std::printf("storage notifications: 100 open/close cycles; descriptors %lld -> %lld; no callbacks after closing\n", static_cast<long long>(before), static_cast<long long>(after));
#else
    std::puts("storage notifications: 100 open/close cycles; no callbacks after closing");
#endif
    return 0;
}
