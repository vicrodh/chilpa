#ifndef CHILPA_FRAME_FENCE_H
#define CHILPA_FRAME_FENCE_H

#include <QtQuick/QQuickItem>
#include <memory>

class FrameFence : public QQuickItem
{
    Q_OBJECT
public:
    explicit FrameFence(QQuickItem *parent = nullptr);
    ~FrameFence() override;
    Q_INVOKABLE void arm(qint64 token);

Q_SIGNALS:
    void presented(qint64 token);

private:
    struct State;
    void attach(QQuickWindow *window);
    void detach();
    void deliver(const std::shared_ptr<State> &state, qint64 token);
    std::shared_ptr<State> m_state;
    QList<QMetaObject::Connection> m_connections;
    qint64 m_requested = 0;
};

extern "C" void chilpa_frame_fence_register_qml_type();

#endif
