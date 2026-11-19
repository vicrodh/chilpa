// Exercise the real scheduler, provider, and QML Image without opening a window.
#define CHILPA_COVER_SCHEDULER_TEST
#include "../src/covers.cpp"
#include <QtCore/QElapsedTimer>
#include <QtCore/QEvent>
#include <QtCore/QEventLoop>
#include <QtCore/QPointer>
#include <QtCore/QTemporaryDir>
#include <QtCore/QTimer>
#include <QtGui/QGuiApplication>
#include <QtQml/QQmlComponent>
#include <QtQml/QQmlEngine>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <mutex>
#include <set>
#include <string>
#include <thread>
#include <vector>

namespace {
std::mutex gateMutex;
std::condition_variable gateChanged;
std::vector<std::string> entered;
std::set<std::string> allowed;
std::string successfulImage;
std::atomic_bool automatic{false};
std::atomic_bool successful{false};
std::atomic_int calls{0};
std::atomic_int active{0};
std::atomic_int maximum{0};
std::atomic_int responsesCreated{0};
std::atomic_int responsesDestroyed{0};
std::atomic_int responsesFinished{0};
std::atomic_int workCompleted{0};
std::atomic_int emissionsAfterEngineDestruction{0};
std::atomic_bool engineDestroyed{false};
bool holdBeforeDelivery = false;
bool beforeDeliveryEntered = false;
bool releaseDelivery = false;

void require(bool condition, const char *message)
{
    if (!condition) {
        std::fprintf(stderr, "FAIL %s\n", message);
        std::exit(1);
    }
}

void waitUntil(const std::function<bool()> &condition, const char *message)
{
    QElapsedTimer timer;
    timer.start();
    while (!condition() && timer.elapsed() < 5'000) {
        QCoreApplication::processEvents(QEventLoop::AllEvents | QEventLoop::WaitForMoreEvents, 50);
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }
    require(condition(), message);
}

bool decode(const char *path, char kind, char *out, unsigned long capacity)
{
    const std::string name = std::string(1, kind) + ':' + path;
    ++calls;
    const int current = ++active;
    int peak = maximum.load();
    while (peak < current && !maximum.compare_exchange_weak(peak, current)) {}
    {
        std::lock_guard lock(gateMutex);
        entered.push_back(name);
    }
    gateChanged.notify_all();
    if (!automatic.load()) {
        std::unique_lock lock(gateMutex);
        gateChanged.wait(lock, [&] { return allowed.find(name) != allowed.end(); });
    }
    const bool result = successful.load();
    if (result && capacity > successfulImage.size())
        std::memcpy(out, successfulImage.c_str(), successfulImage.size() + 1);
    --active;
    gateChanged.notify_all();
    return result && capacity > successfulImage.size();
}

void allow(const std::string &name)
{
    {
        std::lock_guard lock(gateMutex);
        allowed.insert(name);
    }
    gateChanged.notify_all();
}

void allowAllEntered()
{
    std::lock_guard lock(gateMutex);
    allowed.insert(entered.begin(), entered.end());
    gateChanged.notify_all();
}

void waitEntered(const std::string &name)
{
    std::unique_lock lock(gateMutex);
    require(gateChanged.wait_for(lock, std::chrono::seconds(5), [&] {
        return std::find(entered.begin(), entered.end(), name) != entered.end();
    }), "the expected derivative entered Rust");
}

std::string enteredAt(qsizetype index)
{
    std::unique_lock lock(gateMutex);
    require(gateChanged.wait_for(lock, std::chrono::seconds(5), [&] {
        return qsizetype(entered.size()) > index;
    }), "the next derivative entered Rust");
    return entered[index];
}

void reset()
{
    std::unique_lock lock(gateMutex);
    require(gateChanged.wait_for(lock, std::chrono::seconds(5), [] { return active == 0; }),
            "the preceding requests completed");
    entered.clear();
    allowed.clear();
    calls = 0;
    maximum = 0;
    automatic = false;
    successful = false;
    holdBeforeDelivery = false;
    beforeDeliveryEntered = false;
    releaseDelivery = false;
    engineDestroyed = false;
    responsesCreated = 0;
    responsesDestroyed = 0;
    responsesFinished = 0;
    workCompleted = 0;
    emissionsAfterEngineDestruction = 0;
}

void coverTestBeforeDelivery()
{
    std::unique_lock lock(gateMutex);
    if (!holdBeforeDelivery)
        return;
    beforeDeliveryEntered = true;
    gateChanged.notify_all();
    gateChanged.wait(lock, [] { return releaseDelivery; });
}

void coverTestResponseCreated() { ++responsesCreated; }
void coverTestResponseDestroyed() { ++responsesDestroyed; }
void coverTestResponseFinished()
{
    ++responsesFinished;
    if (engineDestroyed.load())
        ++emissionsAfterEngineDestruction;
}
void coverTestWorkCompleted()
{
    ++workCompleted;
}

class Responses {
public:
    explicit Responses(QGuiApplication &application) : m_application(application) {}

    QQuickImageResponse *add(QQuickAsyncImageProvider *provider, const QString &id, bool expectImage = false)
    {
        auto *response = provider->requestImageResponse(id, QSize());
        pointers.emplace_back(response);
        QObject::connect(response, &QQuickImageResponse::finished, &m_application, [this, response, expectImage] {
            if (expectImage) {
                std::unique_ptr<QQuickTextureFactory> texture(response->textureFactory());
                require(texture && texture->textureSize() == QSize(8, 8), "a successful response carries the decoded image");
            }
            ++finished;
            response->deleteLater();
            if (finished == int(pointers.size()))
                loop.quit();
        });
        return response;
    }

    void settle()
    {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 1000);
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }

    void wait()
    {
        QTimer::singleShot(5000, &loop, &QEventLoop::quit);
        if (finished != int(pointers.size()))
            loop.exec();
        require(finished == int(pointers.size()), "every response finishes exactly once");
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
        for (const auto &pointer : pointers)
            require(pointer.isNull(), "finished responses can be deleted safely");
    }

    int finished = 0;
private:
    QGuiApplication &m_application;
    QEventLoop loop;
    std::vector<QPointer<QQuickImageResponse>> pointers;
};

struct Providers {
    std::shared_ptr<CoverScheduler> scheduler = std::make_shared<CoverScheduler>();
    CoverProvider cover{scheduler};
    CoverProvider contained{scheduler, CoverClass::Contained};
    CoverProvider atmosphere{scheduler, CoverClass::Atmosphere};
};

QString coverId(const QString &name) { return QStringLiteral("64/file:///") + name; }
QString atmosphereId(const QString &name) { return QStringLiteral("file:///") + name; }

QObject *createQml(QQmlEngine &engine, const QByteArray &source)
{
    QQmlComponent component(&engine);
    component.setData(source, QUrl(QStringLiteral("file:///cover-scheduler-test.qml")));
    if (component.isError())
        std::fprintf(stderr, "%s\n", qPrintable(component.errorString()));
    QObject *object = component.create();
    require(object, "the QML Image test component is created");
    object->setParent(&engine);
    return object;
}

void addProviders(QQmlEngine &engine, const std::shared_ptr<CoverScheduler> &scheduler)
{
    engine.addImageProvider(QStringLiteral("cover"), new CoverProvider(scheduler));
    engine.addImageProvider(QStringLiteral("contained"), new CoverProvider(scheduler, CoverClass::Contained));
    engine.addImageProvider(QStringLiteral("atmosphere"), new CoverProvider(scheduler, CoverClass::Atmosphere));
}

void waitForScheduler(const std::shared_ptr<CoverScheduler> &scheduler)
{
    waitUntil([&] { return scheduler->pendingCount() == 0 && active == 0; }, "the scheduler becomes idle");
    QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
}

void destroyEngineInState(const char *state, const QByteArray &qml, const std::function<void()> &waitForState,
                          const std::function<void()> &releaseState, int expectedResponses)
{
    auto scheduler = std::make_shared<CoverScheduler>();
    auto engine = std::make_unique<QQmlEngine>();
    addProviders(*engine, scheduler);
    createQml(*engine, qml);
    waitUntil([&] { return responsesCreated == expectedResponses; }, "the engine created every image response");
    waitForState();
    engine.reset();
    engineDestroyed = true;
    releaseState();
    waitForScheduler(scheduler);
    waitUntil([&] { return responsesDestroyed == expectedResponses; }, state);
    require(responsesFinished == expectedResponses, "engine teardown finishes every response once");
    require(emissionsAfterEngineDestruction == 0, "no response emits after engine destruction");
}

void testConcurrentSubscription()
{
    auto scheduler = std::make_shared<CoverScheduler>();
    auto *first = scheduler->request(CoverClass::Contained, coverId(QStringLiteral("race")));
    std::vector<std::thread> askers;
    for (int thread = 0; thread < 4; ++thread) {
        askers.emplace_back([&] {
            for (int request = 0; request < 100; ++request) {
                auto *response = scheduler->request(CoverClass::Cover, coverId(QStringLiteral("race")));
                response->cancel();
                delete response;
            }
        });
    }
    for (auto &asker : askers)
        asker.join();
    waitEntered("c:/race");
    first->cancel();
    delete first;
    allow("c:/race");
    waitForScheduler(scheduler);
    waitUntil([] { return workCompleted >= 1; }, "the concurrent worker returns before test teardown");
}
}

extern "C" bool chilpa_cover(const char *path, unsigned, char *out, unsigned long capacity)
{
    return decode(path, 'c', out, capacity);
}
extern "C" bool chilpa_contained_cover(const char *path, unsigned, char *out, unsigned long capacity)
{
    return decode(path, 'n', out, capacity);
}
extern "C" bool chilpa_atmosphere(const char *path, char *out, unsigned long capacity)
{
    return decode(path, 'a', out, capacity);
}
extern "C" bool chilpa_network_allowed(const char *) { return false; }

int main(int argc, char **argv)
{
    QGuiApplication application(argc, argv);
    QTemporaryDir pictures;
    require(pictures.isValid(), "the test image directory exists");
    successfulImage = pictures.filePath(QStringLiteral("success.png")).toStdString();
    require(QImage(8, 8, QImage::Format_RGB32).save(QString::fromStdString(successfulImage)), "the test image is written");

    if (qEnvironmentVariableIsSet("CHILPA_TSAN")) {
        testConcurrentSubscription();
        std::puts("PASS concurrent cover subscriptions under ThreadSanitizer");
        return 0;
    }

    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId(QStringLiteral("block-a")));
        responses.add(&providers.cover, coverId(QStringLiteral("block-b")));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        responses.add(&providers.cover, coverId(QStringLiteral("old")));
        responses.add(&providers.cover, coverId(QStringLiteral("middle")));
        responses.add(&providers.cover, coverId(QStringLiteral("new")));
        allow("c:/block-a");
        waitEntered("c:/new");
        allow("c:/new");
        waitEntered("c:/middle");
        allow("c:/middle");
        waitEntered("c:/old");
        allow("c:/old");
        allow("c:/block-b");
        responses.wait();
        require(maximum == 2, "the scheduler keeps exactly two workers");
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId(QStringLiteral("block-a")));
        responses.add(&providers.cover, coverId(QStringLiteral("block-b")));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        for (int index = 0; index < 500; ++index)
            responses.add(&providers.cover, coverId(QStringLiteral("burst-%1").arg(index)));
        allow("c:/block-a");
        std::vector<std::string> order;
        for (int position = 0; position < 500; ++position) {
            const std::string name = enteredAt(position + 2);
            order.push_back(name);
            allow(name);
        }
        allow("c:/block-b");
        responses.wait();
        const auto positionOf = [&](int index) {
            const std::string name = "c:/burst-" + std::to_string(index);
            return int(std::find(order.begin(), order.end(), name) - order.begin()) + 1;
        };
        require(positionOf(0) <= 9, "the oldest request gets one of every nine dispatches");
        for (int index = 460; index < 500; ++index)
            require(positionOf(index) <= 45, "the last forty requests stay near the front of a burst");
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId(QStringLiteral("block-a")));
        responses.add(&providers.cover, coverId(QStringLiteral("block-b")));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        responses.add(&providers.cover, coverId(QStringLiteral("old")));
        for (int index = 0; index < 20; ++index)
            responses.add(&providers.cover, coverId(QStringLiteral("pressure-%1").arg(index)));
        responses.add(&providers.cover, coverId(QStringLiteral("old")));
        allow("c:/block-a");
        waitEntered("c:/old");
        for (int index = 0; index < 20; ++index)
            allow("c:/pressure-" + std::to_string(index));
        allow("c:/old");
        allow("c:/block-b");
        responses.wait();
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId(QStringLiteral("block-a")));
        responses.add(&providers.cover, coverId(QStringLiteral("block-b")));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        for (int index = 0; index < 20; ++index)
            responses.add(&providers.cover, coverId(QStringLiteral("cover-%1").arg(index)));
        responses.add(&providers.contained, coverId(QStringLiteral("contained")));
        responses.add(&providers.atmosphere, atmosphereId(QStringLiteral("atmosphere")));
        allow("c:/block-a");
        waitEntered("c:/contained");
        allow("c:/contained");
        waitEntered("n:/contained");
        allow("n:/contained");
        waitEntered("a:/atmosphere");
        allow("a:/atmosphere");
        for (int index = 0; index < 20; ++index)
            allow("c:/cover-" + std::to_string(index));
        allow("c:/block-b");
        responses.wait();
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId(QStringLiteral("block-a")));
        responses.add(&providers.cover, coverId(QStringLiteral("block-b")));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        for (int index = 0; index < 4; ++index)
            responses.add(&providers.cover, coverId(QStringLiteral("same")));
        for (int index = 0; index < 4; ++index)
            responses.add(&providers.contained, coverId(QStringLiteral("same")));
        require(providers.scheduler->pendingCount() == 1, "duplicates share one pending producer");
        allow("c:/block-a");
        waitEntered("c:/same");
        allow("c:/same");
        waitEntered("n:/same");
        responses.add(&providers.contained, coverId(QStringLiteral("same")));
        require(providers.scheduler->pendingCount() == 0, "a late contained asker subscribes to the running producer");
        allow("n:/same");
        allow("c:/block-b");
        responses.wait();
        require(calls == 4, "cover and contained subscribers share one producer");
    }

    reset();
    {
        Providers providers;
        automatic = true;
        successful = true;
        Responses responses(application);
        responses.add(&providers.cover, coverId(QStringLiteral("successful")), true);
        responses.wait();
    }

    reset();
    {
        Providers providers;
        automatic = true;
        successful = true;
        constexpr int count = 140;
        QQmlEngine engine;
        addProviders(engine, providers.scheduler);
        QObject *root = createQml(engine, QByteArrayLiteral(R"(
            import QtQuick
            Item {
                id: root
                property int readyCount: 0
                Repeater {
                    model: 140
                    delegate: Image {
                        width: 8; height: 8; asynchronous: true; cache: false
                        source: "image://cover/64/file:///live-" + index
                        onStatusChanged: if (status === Image.Ready) root.readyCount++
                    }
                }
            }
        )"));
        waitUntil([&] { return root->property("readyCount").toInt() == count; }, "more than 128 live QML Images reach Ready");
        require(responsesCreated == count, "every live QML Image owns a response");
        waitForScheduler(providers.scheduler);
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        auto *running = responses.add(&providers.cover, coverId(QStringLiteral("running")));
        waitEntered("c:/running");
        running->cancel();
        running->cancel();
        responses.settle();
        require(responses.finished == 1, "a cancellation race emits one completion");
        allow("c:/running");
        responses.wait();
        require(calls == 1, "running work is not started twice");
    }

    reset();
    testConcurrentSubscription();

    reset();
    destroyEngineInState("pending engine teardown releases every response", QByteArrayLiteral(R"(
        import QtQuick
        Item { Repeater { model: 3; delegate: Image { asynchronous: true; cache: false; source: "image://cover/64/file:///pending-" + index } } }
    )"), [] {
        waitUntil([] { return active == 2; }, "two engine requests are running");
    }, [] { allowAllEntered(); }, 3);

    reset();
    destroyEngineInState("running engine teardown releases every response", QByteArrayLiteral(R"(
        import QtQuick
        Image { asynchronous: true; cache: false; source: "image://cover/64/file:///running-destruction" }
    )"), [] { waitEntered("c:/running-destruction"); }, [] { allowAllEntered(); }, 1);

    reset();
    successful = true;
    automatic = true;
    {
        std::lock_guard lock(gateMutex);
        holdBeforeDelivery = true;
    }
    destroyEngineInState("delivery engine teardown releases every response", QByteArrayLiteral(R"(
        import QtQuick
        Image { asynchronous: true; cache: false; source: "image://cover/64/file:///delivery-destruction" }
    )"), [] {
        std::unique_lock lock(gateMutex);
        require(gateChanged.wait_for(lock, std::chrono::seconds(5), [] { return beforeDeliveryEntered; }),
                "the response reaches the delivery registry");
    }, [] {
        std::lock_guard lock(gateMutex);
        releaseDelivery = true;
        gateChanged.notify_all();
    }, 1);

    std::puts("PASS cover scheduling, live demand, successful delivery, concurrency, and engine teardown");
}
