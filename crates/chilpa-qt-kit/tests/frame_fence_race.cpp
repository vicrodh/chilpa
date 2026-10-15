#include "frame_fence.h"
#include <QColor>
#include <QGuiApplication>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSemaphore>
#include <QThread>
#include <QTimer>
#include <atomic>
#include <cstdlib>
#include <cstdio>

// I pause real threaded frames after synchronization, without readback.
int main(int argc, char **argv)
{
    QGuiApplication app(argc, argv);
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData("import QtQuick\nRectangle { width: 320; height: 200; color: \"blue\" }", QUrl());
    auto *shell = qobject_cast<QQuickItem *>(component.create());
    if (!shell) { qCritical("Cannot create shell"); return 2; }
    QQuickWindow window;
    window.resize(320, 200);
    shell->setParentItem(window.contentItem());
    FrameFence fence;
    fence.setParentItem(window.contentItem());
    QSemaphore resume;
    std::atomic<bool> gatesEnabled{false};
    std::atomic<int> gatedFrames{0};
    std::atomic<bool> distinctThread{false};
    std::atomic<bool> shellSynced{false};
    bool started = false;
    bool requested = false;
    bool oldQueued = false;
    bool ackGateChecked = false;
    int oldPrepares = 0;
    int fencedPrepares = 0;
    int failures = 0;
    auto check = [&](bool condition, const char *message) {
        std::printf("%s %s\n", condition ? "PASS" : "FAIL", message);
        std::fflush(stdout);
        if (!condition) ++failures;
    };
    QObject::connect(&window, &QQuickWindow::afterSynchronizing, &window, [&] {
        if (gatesEnabled.load())
            shellSynced.store(shell->property("color").value<QColor>() == QColor("green"));
    }, Qt::DirectConnection);
    QObject::connect(&window, &QQuickWindow::beforeRendering, &window, [&] {
        if (!gatesEnabled.load()) return;
        const int frame = gatedFrames.fetch_add(1) + 1;
        if (frame > 2) return;
        distinctThread.store(QThread::currentThread() != app.thread());
        QMetaObject::invokeMethod(&window, [&, frame] {
            if (frame == 1) {
                check(!shellSynced.load(), "old frame synchronized blue before request");
                shell->setProperty("color", QColor("green"));
                requested = true;
                fence.arm(101);
                window.update();
                std::printf("REQUEST token=101 while old frame is paused after sync\n");
                resume.release();
            } else {
                check(shellSynced.load(), "ACK frame synchronized green");
                // I leave the ACK frame unsubmitted while queued GUI work runs.
                QTimer::singleShot(100, &window, [&] {
                    check(distinctThread.load(), "actual threaded render loop");
                    check(oldPrepares == 1, "old first-swap listener starts preparation before ACK submission");
                    check(fencedPrepares == 0, "FrameFence rejects the old frame and waits for ACK submission");
                    ackGateChecked = true;
                    resume.release();
                });
            }
        }, Qt::QueuedConnection);
        if (!resume.tryAcquire(1, 5000)) {
            std::fprintf(stderr, "FAIL render gate timeout frame=%d\n", frame);
            std::_Exit(3);
        }
    }, Qt::DirectConnection);
    QObject::connect(&window, &QQuickWindow::frameSwapped, &window, [&] {
        if (!started) {
            started = true;
            QTimer::singleShot(0, &window, [&] { gatesEnabled.store(true); window.update(); });
        }
        if (requested && !oldQueued) {
            oldQueued = true;
            // I reproduce the old frameSwapped -> Qt.callLater(begin) ordering.
            QMetaObject::invokeMethod(&window, [&] { ++oldPrepares; }, Qt::QueuedConnection);
        }
    }, Qt::QueuedConnection);
    QObject::connect(&fence, &FrameFence::presented, &window, [&](qint64 token) {
        check(token == 101, "FrameFence delivers the requested token");
        check(ackGateChecked, "FrameFence delivers only after ACK render gate is released");
        ++fencedPrepares;
        QTimer::singleShot(50, &window, [&] {
            check(fencedPrepares == 1, "one preparation for the ACK token");
            app.exit(failures ? 1 : 0);
        });
    });
    QTimer::singleShot(10000, &window, [&] { check(false, "test timeout"); resume.release(2); app.exit(4); });
    window.show();
    const int result = app.exec();
    gatesEnabled.store(false);
    resume.release(2);
    window.hide();
    fence.setParentItem(nullptr);
    delete shell;
    return result;
}
