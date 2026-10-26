#ifndef CHILPA_STORAGE_EVENTS_H
#define CHILPA_STORAGE_EVENTS_H
#include <QtCore/QObject>
#include <QtCore/QAbstractNativeEventFilter>
#include <memory>

// I own native notifications only while the device surface is visible.
class StorageEvents : public QObject, public QAbstractNativeEventFilter {
    Q_OBJECT
    Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)
public:
    explicit StorageEvents(QObject *parent = nullptr);
    ~StorageEvents() override;
    bool active() const;
    void setActive(bool active);
    bool nativeEventFilter(const QByteArray &, void *, qintptr *) override;
Q_SIGNALS:
    void activeChanged();
    void changed();
private:
    struct State;
    std::unique_ptr<State> m_state;
};
#endif
