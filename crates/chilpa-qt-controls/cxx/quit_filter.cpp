// The system's own Quit, such as Command+Q or the Dock's menu, reaches the
// application as a quit event. A window that hides when it is closed would
// refuse to close and swallow that quit, so it is stopped here and handed to
// the interface, which asks first or quits.
#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>
#include <QtCore/QObject>

namespace {
class QuitFilter final : public QObject {
public:
    QuitFilter(QObject *parent, bool (*swallow)()) : QObject(parent), swallow_(swallow) {}

protected:
    bool eventFilter(QObject *receiver, QEvent *event) override {
        if (receiver == QCoreApplication::instance() && event->type() == QEvent::Quit && swallow_()) {
            event->ignore();
            return true;
        }
        return false;
    }

private:
    bool (*swallow_)();
};
}

// `swallow` says whether this quit is held back; false lets it through.
extern "C" void chilpa_install_quit_filter(bool (*swallow)())
{
    static QuitFilter *filter = nullptr;
    auto *app = QCoreApplication::instance();
    if (!filter && app) {
        filter = new QuitFilter(app, swallow);
        app->installEventFilter(filter);
    }
}
