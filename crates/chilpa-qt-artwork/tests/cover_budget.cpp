// I exercise the real providers with blocked Rust calls and no window.
#include "../src/covers.cpp"
#include <QtCore/QCoreApplication>
#include <QtCore/QEvent>
#include <QtCore/QEventLoop>
#include <QtCore/QPointer>
#include <QtCore/QSemaphore>
#include <QtCore/QTimer>
#include <cstdio>
#include <cstdlib>
#include <vector>

namespace {
QSemaphore entered;
QSemaphore released;
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

bool decode()
{
    ++calls;
    const int current = ++active;
    int peak = maximum.load();
    while (peak < current && !maximum.compare_exchange_weak(peak, current)) {}
    entered.release();
    released.acquire();
    --active;
    return false;
}

void reset()
{
    require(active == 0, "the preceding requests completed");
    entered.tryAcquire(entered.available());
    released.tryAcquire(released.available());
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
            if (finished == int(pointers.size())) loop.quit();
        });
        return response;
    }

    void wait()
    {
        QTimer::singleShot(5000, &loop, &QEventLoop::quit);
        if (finished != int(pointers.size())) loop.exec();
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
}

extern "C" bool chilpa_cover(const char *, unsigned, char *, unsigned long) { return decode(); }
extern "C" bool chilpa_contained_cover(const char *, unsigned, char *, unsigned long) { return decode(); }
extern "C" bool chilpa_atmosphere(const char *, char *, unsigned long) { return decode(); }
extern "C" bool chilpa_network_allowed(const char *) { return false; }

int main(int argc, char **argv)
{
    QCoreApplication application(argc, argv);
    QQmlEngine engine;
    chilpa_install_covers(&engine);
    auto provider = [&engine](const char *name) {
        auto *found = dynamic_cast<QQuickAsyncImageProvider *>(engine.imageProvider(QString::fromLatin1(name)));
        require(found != nullptr, "the installed provider is asynchronous");
        return found;
    };

    {
        Responses responses(application);
        for (int index = 0; index < 3; ++index) {
            responses.add(provider("cover"), QStringLiteral("64/file:///unused"));
            responses.add(provider("contained"), QStringLiteral("64/file:///unused"));
            responses.add(provider("atmosphere"), QStringLiteral("file:///unused"));
        }
        require(entered.tryAcquire(2, 5000), "two derivative jobs can run together");
        require(!entered.tryAcquire(1, 50), "all three providers share only two workers");
        released.release(9);
        responses.wait();
        require(calls == 9 && maximum == 2, "all derivatives respect the shared pool");
    }

    reset();
    {
        Responses responses(application);
        responses.add(provider("cover"), QStringLiteral("64/file:///unused"));
        responses.add(provider("contained"), QStringLiteral("64/file:///unused"));
        require(entered.tryAcquire(2, 5000), "the workers are occupied");
        auto *cancelled = responses.add(provider("atmosphere"), QStringLiteral("file:///unused"));
        cancelled->cancel();
        released.release(2);
        responses.wait();
        require(calls == 2, "a cancelled queued response skips Rust decoding");
    }

    reset();
    {
        Responses responses(application);
        auto *running = responses.add(provider("cover"), QStringLiteral("64/file:///unused"));
        require(entered.tryAcquire(1, 5000), "the running response entered Rust");
        running->cancel();
        QCoreApplication::processEvents();
        require(responses.finished == 0, "cancellation cannot finish an active Rust call early");
        released.release();
        responses.wait();
        require(calls == 1, "running cancellation waits for its existing work");
    }

    std::puts("PASS shared cover pool, queued cancellation, and running response lifetime");
}
