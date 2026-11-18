#include "pointer_watch.h"
#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>
#include <QtGui/QMouseEvent>
#include <QtQml/qqml.h>
#include <QtQuick/QQuickWindow>

PointerWatch::PointerWatch(QQuickItem *parent) : QQuickItem(parent)
{
    connect(this, &QQuickItem::windowChanged, this, &PointerWatch::attach);
    if (window()) attach(window());
}

void PointerWatch::attach(QQuickWindow *window)
{
    if (m_window) m_window->removeEventFilter(this);
    m_window = window;
    setInside(false);
    if (window) window->installEventFilter(this);
}

void PointerWatch::setInside(bool inside)
{
    if (m_inside == inside) return;
    m_inside = inside;
    Q_EMIT insideChanged();
}

// The window's events pass through untouched: this only reads them.
bool PointerWatch::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == m_window) {
        switch (event->type()) {
        case QEvent::MouseMove:
        case QEvent::MouseButtonPress:
        case QEvent::MouseButtonRelease: {
            const QPointF at = static_cast<QMouseEvent *>(event)->scenePosition();
            setInside(true);
            if (at != m_position || event->type() != QEvent::MouseMove) {
                m_position = at;
                Q_EMIT moved();
            }
            break;
        }
        case QEvent::Leave:
        case QEvent::Hide:
            setInside(false);
            break;
        default:
            break;
        }
    }
    return false;
}

extern "C" void chilpa_pointer_watch_register_qml_type()
{
    static bool registered = false;
    if (registered) return;
    registered = true;
    qmlRegisterType<PointerWatch>("app.chilpa.kit.primitives", 1, 0, "PointerWatch");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_pointer_watch_register_qml_type)
