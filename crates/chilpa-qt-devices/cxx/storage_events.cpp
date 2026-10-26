#include "storage_events.h"
#include <QtCore/QCoreApplication>
#include <QtCore/QTimer>
#include <QtCore/QSocketNotifier>
#include <QtGui/QGuiApplication>
#include <QtQml/qqml.h>
#if defined(Q_OS_LINUX) || defined(Q_OS_ANDROID)
#include <fcntl.h>
#include <sys/socket.h>
#include <linux/netlink.h>
#include <unistd.h>
extern "C" const char *chilpa_mount_events_file();
#elif defined(Q_OS_MACOS)
#include <DiskArbitration/DiskArbitration.h>
#include <IOKit/IOKitLib.h>
#include <dispatch/dispatch.h>
#elif defined(Q_OS_WIN)
#include <windows.h>
#include <dbt.h>
#endif

struct StorageEvents::State {
    StorageEvents *owner;
    bool active = false;
    QTimer debounce;
    QMetaObject::Connection activation;
#if defined(Q_OS_LINUX) || defined(Q_OS_ANDROID)
    int mounts = -1, usb = -1;
    std::unique_ptr<QSocketNotifier> mountNotice, usbNotice;
#elif defined(Q_OS_MACOS)
    DASessionRef disks = nullptr;
    IONotificationPortRef port = nullptr;
    io_iterator_t added = 0, removed = 0;
    static void usbChanged(void *context, io_iterator_t iterator) {
        auto *state = static_cast<State *>(context);
        while (io_object_t device = IOIteratorNext(iterator)) IOObjectRelease(device);
        state->schedule();
    }
    static void diskChanged(DADiskRef, void *context) { static_cast<State *>(context)->schedule(); }
    static void descriptionChanged(DADiskRef, CFArrayRef, void *context) { static_cast<State *>(context)->schedule(); }
#endif
    explicit State(StorageEvents *object) : owner(object) {
        debounce.setSingleShot(true);
        debounce.setInterval(750);
        QObject::connect(&debounce, &QTimer::timeout, owner, [this] { if (active) Q_EMIT owner->changed(); });
    }
    void schedule() { if (active) debounce.start(); }
    void start() {
        activation = QObject::connect(qGuiApp, &QGuiApplication::applicationStateChanged, owner,
            [this](Qt::ApplicationState value) { if (value == Qt::ApplicationActive) schedule(); });
#if defined(Q_OS_LINUX) || defined(Q_OS_ANDROID)
        mounts = ::open(chilpa_mount_events_file(), O_RDONLY | O_CLOEXEC | O_NONBLOCK);
        if (mounts >= 0) {
            mountNotice = std::make_unique<QSocketNotifier>(mounts, QSocketNotifier::Exception);
            QObject::connect(mountNotice.get(), &QSocketNotifier::activated, owner, [this] {
                char data[4096];
                ::lseek(mounts, 0, SEEK_SET);
                for (int total = 0; total < 4 * 1024 * 1024;) {
                    const auto size = ::read(mounts, data, sizeof(data));
                    if (size <= 0) break;
                    total += static_cast<int>(size);
                }
                schedule();
            });
        }
        usb = ::socket(AF_NETLINK, SOCK_DGRAM | SOCK_CLOEXEC | SOCK_NONBLOCK, NETLINK_KOBJECT_UEVENT);
        if (usb >= 0) {
            sockaddr_nl address{}; address.nl_family = AF_NETLINK; address.nl_groups = 1;
            if (::bind(usb, reinterpret_cast<sockaddr *>(&address), sizeof(address)) != 0) { ::close(usb); usb = -1; }
        }
        if (usb >= 0) {
            usbNotice = std::make_unique<QSocketNotifier>(usb, QSocketNotifier::Read);
            QObject::connect(usbNotice.get(), &QSocketNotifier::activated, owner, [this] {
                // I bound each drain; the notifier delivers any remaining datagrams later.
                for (int count = 0; count < 64; ++count) {
                    char data[8192]; sockaddr_nl sender{};
                    iovec buffer{data, sizeof(data)};
                    msghdr message{}; message.msg_name = &sender; message.msg_namelen = sizeof(sender);
                    message.msg_iov = &buffer; message.msg_iovlen = 1;
                    const auto size = ::recvmsg(usb, &message, MSG_DONTWAIT);
                    if (size <= 0) break;
                    if (sender.nl_pid != 0 || message.msg_flags & MSG_TRUNC) continue;
                    const QByteArray event(data, static_cast<int>(size));
                    if (event.contains("SUBSYSTEM=usb\0") || event.contains("SUBSYSTEM=block\0")) schedule();
                }
            });
        }
#elif defined(Q_OS_MACOS)
        disks = DASessionCreate(kCFAllocatorDefault);
        if (disks) {
            DARegisterDiskAppearedCallback(disks, nullptr, diskChanged, this);
            DARegisterDiskDisappearedCallback(disks, nullptr, diskChanged, this);
            DARegisterDiskDescriptionChangedCallback(disks, nullptr, nullptr, descriptionChanged, this);
            DASessionSetDispatchQueue(disks, dispatch_get_main_queue());
        }
        port = IONotificationPortCreate(kIOMainPortDefault);
        if (port) {
            IONotificationPortSetDispatchQueue(port, dispatch_get_main_queue());
            IOServiceAddMatchingNotification(port, kIOFirstMatchNotification, IOServiceMatching("IOUSBHostDevice"), usbChanged, this, &added);
            IOServiceAddMatchingNotification(port, kIOTerminatedNotification, IOServiceMatching("IOUSBHostDevice"), usbChanged, this, &removed);
            // I drain the initial population to arm notifications without a duplicate discovery.
            while (io_object_t device = IOIteratorNext(added)) IOObjectRelease(device);
            while (io_object_t device = IOIteratorNext(removed)) IOObjectRelease(device);
        }
#elif defined(Q_OS_WIN)
        QCoreApplication::instance()->installNativeEventFilter(owner);
#endif
    }
    void stop() {
        debounce.stop(); QObject::disconnect(activation);
#if defined(Q_OS_LINUX) || defined(Q_OS_ANDROID)
        mountNotice.reset(); usbNotice.reset();
        if (mounts >= 0) ::close(mounts);
        if (usb >= 0) ::close(usb);
        mounts = usb = -1;
#elif defined(Q_OS_MACOS)
        if (disks) {
            DAUnregisterCallback(disks, reinterpret_cast<void *>(diskChanged), this);
            DAUnregisterCallback(disks, reinterpret_cast<void *>(descriptionChanged), this);
            DASessionSetDispatchQueue(disks, nullptr); CFRelease(disks); disks = nullptr;
        }
        if (port) { IONotificationPortSetDispatchQueue(port, nullptr); IONotificationPortDestroy(port); port = nullptr; }
        if (added) IOObjectRelease(added);
        if (removed) IOObjectRelease(removed);
        added = removed = 0;
#elif defined(Q_OS_WIN)
        if (auto *application = QCoreApplication::instance()) application->removeNativeEventFilter(owner);
#endif
    }
};
StorageEvents::StorageEvents(QObject *parent) : QObject(parent), m_state(std::make_unique<State>(this)) {}
StorageEvents::~StorageEvents() { m_state->active = false; m_state->stop(); }
bool StorageEvents::active() const { return m_state->active; }
void StorageEvents::setActive(bool value) {
    if (value == m_state->active) return;
    m_state->active = value;
    if (value) m_state->start(); else m_state->stop();
    Q_EMIT activeChanged();
}
bool StorageEvents::nativeEventFilter(const QByteArray &, void *raw, qintptr *) {
#if defined(Q_OS_WIN)
    const auto *message = static_cast<MSG *>(raw);
    if (m_state->active && message->message == WM_DEVICECHANGE &&
        (message->wParam == DBT_DEVICEARRIVAL || message->wParam == DBT_DEVICEREMOVECOMPLETE || message->wParam == DBT_DEVNODES_CHANGED)) m_state->schedule();
#else
    Q_UNUSED(raw)
#endif
    return false;
}
extern "C" void chilpa_storage_events_register_qml_type() {
    static bool registered = false;
    if (registered) return;
    registered = true;
    qmlRegisterType<StorageEvents>("app.chilpa.devices", 1, 0, "StorageEvents");
}
Q_COREAPP_STARTUP_FUNCTION(chilpa_storage_events_register_qml_type)
