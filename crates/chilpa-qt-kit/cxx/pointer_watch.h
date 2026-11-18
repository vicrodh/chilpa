#ifndef CHILPA_POINTER_WATCH_H
#define CHILPA_POINTER_WATCH_H

#include <QtCore/QPointF>
#include <QtCore/QPointer>
#include <QtQuick/QQuickItem>

// Where the pointer is in its window, read from the window itself. Hover
// reaches an item's handler only through the items above it, and an area
// that takes hover anywhere under the pointer can keep it from a handler
// on what holds everything: this hears every move, and takes none.
class PointerWatch : public QQuickItem
{
    Q_OBJECT
    Q_PROPERTY(QPointF position READ position NOTIFY moved)
    Q_PROPERTY(bool inside READ inside NOTIFY insideChanged)
public:
    explicit PointerWatch(QQuickItem *parent = nullptr);
    QPointF position() const { return m_position; }
    bool inside() const { return m_inside; }

Q_SIGNALS:
    void moved();
    void insideChanged();

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    void attach(QQuickWindow *window);
    void setInside(bool inside);
    QPointer<QQuickWindow> m_window;
    QPointF m_position;
    bool m_inside = false;
};

extern "C" void chilpa_pointer_watch_register_qml_type();

#endif
