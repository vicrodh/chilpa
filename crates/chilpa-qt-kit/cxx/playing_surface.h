#pragma once
#include <QtQuick/QQuickItem>
#include <QtCore/QPointer>
#include <QtGui/QColor>
#include <QtGui/QWindow>

class PlayingSurface : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)
    Q_PROPERTY(qreal phase READ phase WRITE setPhase NOTIFY phaseChanged)
    Q_PROPERTY(qreal barScale READ barScale WRITE setBarScale NOTIFY barScaleChanged)
    Q_PROPERTY(QColor tint READ tint WRITE setTint NOTIFY tintChanged)
    Q_PROPERTY(QWindow *surface READ surface NOTIFY surfaceChanged)
public:
    explicit PlayingSurface(QQuickItem *parent = nullptr);
    ~PlayingSurface() override;
    bool active() const { return m_active; }
    qreal phase() const { return m_phase; }
    qreal barScale() const { return m_scale; }
    QColor tint() const { return m_tint; }
    QWindow *surface() const { return m_surface; }
    void setActive(bool value);
    void setPhase(qreal value);
    void setBarScale(qreal value);
    void setTint(const QColor &value);
signals:
    void activeChanged();
    void phaseChanged();
    void barScaleChanged();
    void tintChanged();
    void surfaceChanged();
protected:
    void updatePolish() override;
private:
    void watchWindow();
    void releaseSurface();
    bool exposed() const;
    bool m_active = false;
    qreal m_phase = 0;
    qreal m_scale = 1;
    QColor m_tint;
    QPointer<QWindow> m_surface;
    QMetaObject::Connection m_frame;
};
