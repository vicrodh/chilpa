// Exercise the real scheduler with blocked Rust calls and no window.
#include "../src/covers.cpp"
#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>
#include <QtCore/QEventLoop>
#include <QtCore/QPointer>
#include <QtCore/QTimer>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <mutex>
#include <set>
#include <string>
#include <vector>

namespace {
std::mutex gateMutex;
std::condition_variable gateChanged;
std::vector<std::string> entered;
std::set<std::string> allowed;
std::atomic_int calls{0};
std::atomic_int active{0};
std::atomic_int maximum{0};

void require(bool condition, const char *message)
{
    if (!condition) {
        std::fprintf(stderr, "FAIL %s\n", message);
        std::exit(1);
    }
}

bool decode(const char *path, char kind)
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
    {
        std::unique_lock lock(gateMutex);
        gateChanged.wait(lock, [&] { return allowed.find(name) != allowed.end(); });
    }
    --active;
    return false;
}

void allow(const std::string &name)
{
    {
        std::lock_guard lock(gateMutex);
        allowed.insert(name);
    }
    gateChanged.notify_all();
}

void allowAll()
{
    std::vector<std::string> names;
    {
        std::lock_guard lock(gateMutex);
        names = entered;
    }
    for (const auto &name : names)
        allow(name);
}

void waitEntered(const std::string &name)
{
    std::unique_lock lock(gateMutex);
    require(gateChanged.wait_for(lock, std::chrono::seconds(5), [&] {
        return std::find(entered.begin(), entered.end(), name) != entered.end();
    }), "the expected derivative entered Rust");
}

void reset()
{
    require(active == 0, "the preceding requests completed");
    std::lock_guard lock(gateMutex);
    entered.clear();
    allowed.clear();
    calls = 0;
    maximum = 0;
}

class Responses {
public:
    explicit Responses(QCoreApplication &application) : m_application(application) {}

    QQuickImageResponse *add(QQuickAsyncImageProvider *provider, const QString &id)
    {
        auto *response = provider->requestImageResponse(id, QSize());
        pointers.emplace_back(response);
        QObject::connect(response, &QQuickImageResponse::finished, &m_application, [this, response] {
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
    QCoreApplication &m_application;
    QEventLoop loop;
    std::vector<QPointer<QQuickImageResponse>> pointers;
};

struct Providers {
    std::shared_ptr<CoverScheduler> scheduler = std::make_shared<CoverScheduler>();
    CoverProvider cover{scheduler};
    CoverProvider contained{scheduler, CoverClass::Contained};
    CoverProvider atmosphere{scheduler, CoverClass::Atmosphere};
};

QString coverId(const char *name) { return QStringLiteral("64/file:///") + QString::fromLatin1(name); }
QString atmosphereId(const char *name) { return QStringLiteral("file:///") + QString::fromLatin1(name); }
}

extern "C" bool chilpa_cover(const char *path, unsigned, char *, unsigned long) { return decode(path, 'c'); }
extern "C" bool chilpa_contained_cover(const char *path, unsigned, char *, unsigned long) { return decode(path, 'n'); }
extern "C" bool chilpa_atmosphere(const char *path, char *, unsigned long) { return decode(path, 'a'); }
extern "C" bool chilpa_network_allowed(const char *) { return false; }

int main(int argc, char **argv)
{
    QCoreApplication application(argc, argv);

    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId("block-a"));
        responses.add(&providers.cover, coverId("block-b"));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        responses.add(&providers.cover, coverId("old"));
        responses.add(&providers.cover, coverId("middle"));
        responses.add(&providers.cover, coverId("new"));
        allow("c:/block-a");
        waitEntered("c:/new");
        allow("c:/new");
        waitEntered("c:/middle");
        allowAll();
        allow("c:/old");
        responses.wait();
        require(maximum == 2, "the scheduler keeps exactly two workers");
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId("block-a"));
        responses.add(&providers.cover, coverId("block-b"));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        responses.add(&providers.cover, coverId("old"));
        for (int index = 1; index <= 9; ++index)
            responses.add(&providers.cover, coverId(QString("new-%1").arg(index).toLatin1().constData()));
        allow("c:/block-a");
        for (int index = 9; index >= 2; --index) {
            const std::string name = "c:/new-" + std::to_string(index);
            waitEntered(name);
            allow(name);
        }
        waitEntered("c:/old");
        allowAll();
        allow("c:/new-1");
        responses.wait();
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId("block-a"));
        responses.add(&providers.cover, coverId("block-b"));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        for (int index = 0; index < 20; ++index)
            responses.add(&providers.cover, coverId(QString("cover-%1").arg(index).toLatin1().constData()));
        responses.add(&providers.contained, coverId("contained"));
        responses.add(&providers.atmosphere, atmosphereId("atmosphere"));
        allow("c:/block-a");
        waitEntered("c:/contained");
        allow("c:/contained");
        waitEntered("n:/contained");
        allow("n:/contained");
        waitEntered("a:/atmosphere");
        allowAll();
        for (int index = 0; index < 20; ++index)
            allow("c:/cover-" + std::to_string(index));
        responses.wait();
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId("block-a"));
        responses.add(&providers.cover, coverId("block-b"));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        for (int index = 0; index < 4; ++index)
            responses.add(&providers.cover, coverId("same"));
        for (int index = 0; index < 4; ++index)
            responses.add(&providers.contained, coverId("same"));
        require(providers.scheduler->pendingCount() == 1, "duplicates share one pending producer");
        allow("c:/block-a");
        waitEntered("c:/same");
        allow("c:/same");
        waitEntered("n:/same");
        responses.add(&providers.contained, coverId("same"));
        require(providers.scheduler->pendingCount() == 0, "a late contained asker subscribes to the running producer");
        allow("n:/same");
        allow("c:/block-b");
        responses.wait();
        require(calls == 4, "cover and contained subscribers do not occupy waiting worker slots");
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        responses.add(&providers.cover, coverId("block-a"));
        responses.add(&providers.cover, coverId("block-b"));
        waitEntered("c:/block-a");
        waitEntered("c:/block-b");
        std::vector<QQuickImageResponse *> pending;
        for (int index = 0; index <= CoverScheduler::MaxPending; ++index)
            pending.push_back(responses.add(&providers.cover, coverId(QString("pending-%1").arg(index).toLatin1().constData())));
        require(providers.scheduler->pendingCount() == CoverScheduler::MaxPending, "the pending set stays bounded");
        responses.settle();
        require(responses.finished == 1, "the least recent overflow response finishes explicitly");
        for (qsizetype index = 1; index < qsizetype(pending.size()); ++index)
            pending[index]->cancel();
        allowAll();
        responses.wait();
        require(calls == 2, "cancelled pending responses never enter Rust");
    }

    reset();
    {
        Providers providers;
        Responses responses(application);
        auto *running = responses.add(&providers.cover, coverId("running"));
        waitEntered("c:/running");
        running->cancel();
        running->cancel();
        responses.settle();
        require(responses.finished == 1, "a cancellation race schedules one completion");
        allow("c:/running");
        responses.wait();
        require(calls == 1, "running work is not started twice");
    }

    std::puts("PASS bounded newest-first cover scheduling, aging, fairness, cancellation, and deduplication");
}
