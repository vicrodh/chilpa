#include "frame_fence.h"
#include <QtCore/QCoreApplication>
#include <QtQml/qqml.h>
#include <QtQuick/QQuickWindow>
#include <atomic>
#include <mutex>

struct FrameFence::State {
    std::atomic<qint64> requested{0};
    qint64 synchronized = 0;
    qint64 sent = 0;
    std::mutex delivery;
    FrameFence *recipient = nullptr;
};

FrameFence::FrameFence(QQuickItem *parent) : QQuickItem(parent)
{
    connect(this, &QQuickItem::windowChanged, this, &FrameFence::attach);
    if (window()) attach(window());
}

FrameFence::~FrameFence() { detach(); }

void FrameFence::detach()
{
    if (m_state) {
        std::lock_guard<std::mutex> guard(m_state->delivery);
        m_state->recipient = nullptr;
    }
    for (const auto &connection : m_connections) disconnect(connection);
    m_connections.clear();
    m_state.reset();
}

void FrameFence::attach(QQuickWindow *window)
{
    detach();
    if (!window) return;
    const auto state = std::make_shared<State>();
    state->recipient = this;
    state->requested.store(m_requested);
    m_state = state;
    m_connections.append(connect(window, &QQuickWindow::afterSynchronizing, this, [state] {
        // I read the requested token only while the GUI thread is blocked.
        state->synchronized = state->requested.load();
    }, Qt::DirectConnection));
    m_connections.append(connect(window, &QQuickWindow::frameSwapped, this, [state] {
        const qint64 token = state->synchronized;
        if (token <= 0 || token == state->sent) return;
        state->sent = token;
        std::lock_guard<std::mutex> guard(state->delivery);
        if (FrameFence *recipient = state->recipient) {
            QMetaObject::invokeMethod(recipient, [recipient, state, token] {
                recipient->deliver(state, token);
            }, Qt::QueuedConnection);
        }
    }, Qt::DirectConnection));
    if (m_requested > 0) window->update();
}

void FrameFence::arm(qint64 token)
{
    m_requested = token;
    if (m_state) m_state->requested.store(token);
    if (window()) window()->update();
}

void FrameFence::deliver(const std::shared_ptr<State> &state, qint64 token)
{
    if (m_state == state && m_requested == token) Q_EMIT presented(token);
}

extern "C" void chilpa_frame_fence_register_qml_type()
{
    static bool registered = false;
    if (registered) return;
    registered = true;
    qmlRegisterType<FrameFence>("app.chilpa.kit", 1, 0, "FrameFence");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_frame_fence_register_qml_type)
