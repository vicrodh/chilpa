// The window's graphics probes, ported from QBZ's cxx/qt_vulkan_probe.cpp:
// the list of Qt's own Vulkan devices, and the tiny window a startup child
// opens to prove that a backend presents frames.

#include <QtCore/QByteArray>
#include <QtCore/QCoreApplication>
#include <QtCore/QEventLoop>
#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QTimer>
#include <QtCore/QVector>
#include <QtCore/QVersionNumber>
#include <QtGui/QGuiApplication>
#include <QtGui/qtguiglobal.h>
#include <QtQuick/QQuickWindow>

#if QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>) && __has_include(<private/qvulkandefaultinstance_p.h>)
#define CHILPA_QT_VULKAN 1
#include <QtGui/QVulkanFunctions>
#include <private/qvulkandefaultinstance_p.h>
#endif

namespace {

QByteArray serializedDevices("[]");

#ifdef CHILPA_QT_VULKAN
QByteArray deviceUuid(QVulkanFunctions *functions, VkPhysicalDevice device,
                      const QVulkanInstance *instance)
{
    if (instance->apiVersion() < QVersionNumber(1, 1))
        return {};

    VkPhysicalDeviceIDProperties id{};
    id.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES;
    VkPhysicalDeviceProperties2 properties{};
    properties.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2;
    properties.pNext = &id;
    functions->vkGetPhysicalDeviceProperties2(device, &properties);

    bool any = false;
    for (const auto byte : id.deviceUUID)
        any = any || byte != 0;
    if (!any)
        return {};
    return QByteArray(reinterpret_cast<const char *>(id.deviceUUID), VK_UUID_SIZE).toHex();
}
#endif

} // namespace

// The devices of the very QVulkanInstance Qt Quick later hands to QRhi, not
// of a second instance of our own: hybrid-GPU layers may order them
// differently per instance, and only Qt's order makes
// QT_VK_PHYSICAL_DEVICE_INDEX an identity instead of a guess. Called after
// the application object exists and before the first window; "[]" where
// there is no usable Vulkan.
extern "C" const char *chilpa_vulkan_devices_json()
{
    serializedDevices = "[]";

#ifdef CHILPA_QT_VULKAN
    if (QCoreApplication::instance() == nullptr)
        return serializedDevices.constData();

    QVulkanInstance *instance = QVulkanDefaultInstance::instance();
    if (instance == nullptr || !instance->isValid())
        return serializedDevices.constData();

    QVulkanFunctions *functions = instance->functions();
    if (functions == nullptr)
        return serializedDevices.constData();

    uint32_t count = 0;
    if (functions->vkEnumeratePhysicalDevices(instance->vkInstance(), &count, nullptr)
            != VK_SUCCESS || count == 0) {
        return serializedDevices.constData();
    }

    QVector<VkPhysicalDevice> devices(static_cast<qsizetype>(count));
    if (functions->vkEnumeratePhysicalDevices(instance->vkInstance(), &count, devices.data())
            != VK_SUCCESS) {
        return serializedDevices.constData();
    }

    QJsonArray result;
    for (uint32_t index = 0; index < count; ++index) {
        VkPhysicalDeviceProperties properties{};
        functions->vkGetPhysicalDeviceProperties(devices.at(index), &properties);

        QJsonObject item;
        item.insert(QStringLiteral("index"), static_cast<int>(index));
        item.insert(QStringLiteral("name"), QString::fromUtf8(properties.deviceName).trimmed());
        item.insert(QStringLiteral("vendor"), static_cast<qint64>(properties.vendorID));
        item.insert(QStringLiteral("device"), static_cast<qint64>(properties.deviceID));
        item.insert(QStringLiteral("type"), static_cast<int>(properties.deviceType));
        item.insert(QStringLiteral("uuid"),
                    QString::fromLatin1(deviceUuid(functions, devices.at(index), instance)));
        result.append(item);
    }
    serializedDevices = QJsonDocument(result).toJson(QJsonDocument::Compact);
#endif

    return serializedDevices.constData();
}

// The path the real window needs, inside the disposable child: a Qt Quick
// window has to present real frames to the active compositor. Creating a
// device succeeds even where the first buffer import later kills the
// connection, so only presented frames count. If the compositor kills this
// process, the parent is alive and falls back.
static int presentationPreflight(bool autoBackend)
{
    if (qobject_cast<QGuiApplication *>(QCoreApplication::instance()) == nullptr)
        return 71;

    QQuickWindow window;
    window.setTitle(QStringLiteral("Chilpa graphics preflight"));
    window.setFlags(Qt::Tool | Qt::FramelessWindowHint | Qt::WindowDoesNotAcceptFocus);
    window.resize(2, 2);
    window.setColor(Qt::transparent);
    // Next to invisible, yet a real buffer commit: an offscreen window would
    // miss the very failure this exists to catch.
    window.setOpacity(0.01);

    QEventLoop loop;
    QTimer pulse;
    QTimer settle;
    QTimer deadline;
    pulse.setInterval(16);
    settle.setSingleShot(true);
    deadline.setSingleShot(true);

    int frames = 0;
    int result = 76; // the deadline passed on screen: no usable presentation

    QObject::connect(&pulse, &QTimer::timeout, &window, &QQuickWindow::update);
    QObject::connect(&window, &QQuickWindow::frameSwapped, &loop, [&] {
        ++frames;
        if (autoBackend && frames >= 3) {
            // Three presentations, not only a context: and no fixed wait on
            // the healthy path.
            result = 0;
            loop.quit();
        } else if (!autoBackend && frames == 1) {
            settle.start(700);
        }
        window.update();
    });
    QObject::connect(&window, &QQuickWindow::sceneGraphError, &loop,
                     [&](QQuickWindow::SceneGraphError, const QString &) {
        result = 74;
        loop.quit();
    });
    // A window the compositor does not show (a locked screen, another
    // desktop) gets no frame callback, and Qt then takes it for unexposed:
    // missing frames say nothing about the backend there, so the answer is
    // 77, not a failure.
    const auto missing = [&](int failure) { return window.isExposed() ? failure : 77; };
    QObject::connect(&settle, &QTimer::timeout, &loop, [&] {
        // Two swaps and a grace: long enough for an asynchronous import
        // error from the compositor to arrive.
        result = frames >= 2 ? 0 : missing(75);
        loop.quit();
    });
    QObject::connect(&deadline, &QTimer::timeout, &loop, [&] {
        result = missing(76);
        loop.quit();
    });

    window.show();
    window.update();
    pulse.start();
    deadline.start(5000);
    loop.exec();

    pulse.stop();
    settle.stop();
    deadline.stop();
    window.hide();
    window.releaseResources();
    QCoreApplication::processEvents(QEventLoop::AllEvents, 50);
    return result;
}

// A saved GPU, with Vulkan and its index already in the environment.
extern "C" int chilpa_gpu_preflight_window()
{
    return presentationPreflight(false);
}

// Qt's own default (OpenGL on Linux), or software chosen before the
// application object.
extern "C" int chilpa_auto_preflight_window()
{
    return presentationPreflight(true);
}
