// The window's covers, at the size they are drawn (frontend ADR-0014,
// rule 5): an image provider behind `image://cover/<size>/<path>`.
//
// All three providers share two workers and one bounded scheduler. Pending
// work is newest-first within its class; after eight other dispatches the
// oldest request in that class ages to the front. Classes take round-robin
// turns so contained covers and atmosphere cannot starve row covers or be
// starved by them. When covers are pending, non-cover work occupies at most
// one worker so headers and immersive views cannot take both cover slots.

#include <QtCore/QHash>
#include <QtCore/QMetaObject>
#include <QtCore/QMutex>
#include <QtCore/QThreadPool>
#include <QtCore/QUrl>
#include <QtGui/QImage>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlEngine>
#include <QtQml/QQmlNetworkAccessManagerFactory>
#include <QtQuick/QQuickImageProvider>
#include <algorithm>
#include <atomic>
#include <memory>
#include <utility>
#include <vector>

extern "C" bool chilpa_cover(const char *path, unsigned size, char *out, unsigned long capacity);
extern "C" bool chilpa_contained_cover(const char *path, unsigned size, char *out, unsigned long capacity);
extern "C" bool chilpa_atmosphere(const char *path, char *out, unsigned long capacity);
extern "C" bool chilpa_network_allowed(const char *address);

namespace {

class NetworkAccess : public QNetworkAccessManager
{
public:
    using QNetworkAccessManager::QNetworkAccessManager;
protected:
    QNetworkReply *createRequest(Operation op, const QNetworkRequest &request, QIODevice *body) override
    {
        const QUrl url = request.url();
        if ((url.scheme() == QStringLiteral("https") || url.scheme() == QStringLiteral("http"))
            && !chilpa_network_allowed(url.toEncoded().constData()))
            return QNetworkAccessManager::createRequest(op, QNetworkRequest(QUrl()), body);
        return QNetworkAccessManager::createRequest(op, request, body);
    }
};

class NetworkFactory : public QQmlNetworkAccessManagerFactory
{
public:
    QNetworkAccessManager *create(QObject *parent) override { return new NetworkAccess(parent); }
};

enum class CoverClass : int { Cover = 0, Contained = 1, Atmosphere = 2 };
enum class WorkPhase { Base, Contained, Completing };
class CoverScheduler;

class CoverResponse : public QQuickImageResponse
{
public:
    explicit CoverResponse(std::weak_ptr<CoverScheduler> scheduler) : m_scheduler(std::move(scheduler)) {}

    void cancel() override;
    QQuickTextureFactory *textureFactory() const override { return QQuickTextureFactory::textureFactoryForImage(m_image); }
    QString errorString() const override { return m_error; }
    bool cancelled() const { return m_cancelled.load(std::memory_order_acquire); }

    void finish(QImage image = {}, QString error = {})
    {
        if (m_completionScheduled.exchange(true, std::memory_order_acq_rel))
            return;
        QMetaObject::invokeMethod(this, [this, image = std::move(image), error = std::move(error)]() mutable {
            m_image = std::move(image);
            m_error = std::move(error);
            emit finished();
        }, Qt::QueuedConnection);
    }

private:
    std::weak_ptr<CoverScheduler> m_scheduler;
    QImage m_image;
    QString m_error;
    std::atomic_bool m_cancelled{false};
    std::atomic_bool m_completionScheduled{false};
};

struct CoverWork {
    struct Subscriber { CoverResponse *response; CoverClass kind; };
    QString key;
    QString id;
    CoverClass kind;
    WorkPhase phase = WorkPhase::Base;
    bool containedOnly = false;
    quint64 sequence = 0;
    quint64 dispatchAtEnqueue = 0;
    std::vector<Subscriber> subscribers;
};

class CoverJob : public QRunnable
{
public:
    CoverJob(std::shared_ptr<CoverScheduler> scheduler, std::shared_ptr<CoverWork> work)
        : m_scheduler(std::move(scheduler)), m_work(std::move(work)) {}
    void run() override;
private:
    std::shared_ptr<CoverScheduler> m_scheduler;
    std::shared_ptr<CoverWork> m_work;
};

class CoverScheduler : public std::enable_shared_from_this<CoverScheduler>
{
public:
    static constexpr qsizetype MaxPending = 128;
    static constexpr quint64 AgingDispatches = 8;

    CoverScheduler() { m_pool.setMaxThreadCount(2); }
    ~CoverScheduler() { m_pool.waitForDone(); }

    CoverResponse *request(CoverClass kind, const QString &id)
    {
        auto *response = new CoverResponse(weak_from_this());
        std::vector<CoverWork::Subscriber> displaced;
        {
            QMutexLocker lock(&m_mutex);
            QString key = kind == CoverClass::Atmosphere ? QStringLiteral("atmosphere:") + id
                                                         : QStringLiteral("derivative:") + id;
            if (const auto existing = m_byKey.value(key)) {
                if (kind != CoverClass::Contained || existing->phase != WorkPhase::Completing) {
                    existing->subscribers.push_back({response, kind});
                    if (kind == CoverClass::Cover)
                        existing->kind = CoverClass::Cover;
                    return response;
                }
                key = QStringLiteral("contained:") + id;
                if (const auto contained = m_byKey.value(key)) {
                    contained->subscribers.push_back({response, kind});
                    return response;
                }
            }
            if (m_pending.size() >= MaxPending) {
                auto oldest = std::min_element(m_pending.begin(), m_pending.end(), [](const auto &left, const auto &right) {
                    return left->sequence < right->sequence;
                });
                displaced = std::move((*oldest)->subscribers);
                m_byKey.remove((*oldest)->key);
                m_pending.erase(oldest);
            }
            auto work = std::make_shared<CoverWork>();
            work->key = key;
            work->id = id;
            work->kind = kind;
            work->containedOnly = key.startsWith(QStringLiteral("contained:"));
            work->sequence = ++m_sequence;
            work->dispatchAtEnqueue = m_dispatches;
            work->subscribers.push_back({response, kind});
            m_pending.push_back(work);
            m_byKey.insert(key, work);
            dispatchLocked();
        }
        for (const auto &entry : displaced)
            entry.response->finish({}, QStringLiteral("cover request superseded"));
        return response;
    }

    void cancel(CoverResponse *response)
    {
        bool found = false;
        {
            QMutexLocker lock(&m_mutex);
            for (auto pending = m_pending.begin(); pending != m_pending.end(); ++pending) {
                auto &subscribers = (*pending)->subscribers;
                const auto subscriber = std::find_if(subscribers.begin(), subscribers.end(), [response](const auto &entry) {
                    return entry.response == response;
                });
                if (subscriber == subscribers.end())
                    continue;
                subscribers.erase(subscriber);
                if (subscribers.empty()) {
                    m_byKey.remove((*pending)->key);
                    m_pending.erase(pending);
                }
                found = true;
                break;
            }
            if (!found) {
                for (const auto &work : std::as_const(m_running)) {
                    const auto subscriber = std::find_if(work->subscribers.begin(), work->subscribers.end(), [response](const auto &entry) {
                        return entry.response == response;
                    });
                    if (subscriber == work->subscribers.end())
                        continue;
                    work->subscribers.erase(subscriber);
                    found = true;
                    break;
                }
            }
        }
        if (found)
            response->finish({}, QStringLiteral("cover request cancelled"));
    }

    void execute(const std::shared_ptr<CoverWork> &work)
    {
        {
            QMutexLocker lock(&m_mutex);
            if (work->subscribers.empty()) {
                m_byKey.remove(work->key);
                m_running.erase(std::find(m_running.begin(), m_running.end(), work));
                dispatchLocked();
                return;
            }
        }
        QByteArray coverOut(4096, '\0');
        QByteArray containedOut(4096, '\0');
        if (work->kind == CoverClass::Atmosphere) {
            const bool made = make(work, CoverClass::Atmosphere, coverOut);
            complete(work, made ? QString::fromUtf8(coverOut.constData()) : QString(), {});
            return;
        }
        if (work->containedOnly) {
            const bool made = make(work, CoverClass::Contained, containedOut);
            complete(work, {}, made ? QString::fromUtf8(containedOut.constData()) : QString());
            return;
        }
        const bool coverMade = make(work, CoverClass::Cover, coverOut);
        bool makeContained = false;
        {
            QMutexLocker lock(&m_mutex);
            makeContained = std::any_of(work->subscribers.begin(), work->subscribers.end(), [](const auto &entry) {
                return entry.kind == CoverClass::Contained;
            });
            work->phase = makeContained ? WorkPhase::Contained : WorkPhase::Completing;
        }
        const bool containedMade = makeContained && make(work, CoverClass::Contained, containedOut);
        {
            QMutexLocker lock(&m_mutex);
            work->phase = WorkPhase::Completing;
        }
        complete(work, coverMade ? QString::fromUtf8(coverOut.constData()) : QString(),
                 containedMade ? QString::fromUtf8(containedOut.constData()) : QString());
    }

    qsizetype pendingCount() const
    {
        QMutexLocker lock(&m_mutex);
        return m_pending.size();
    }

private:
    bool make(const std::shared_ptr<CoverWork> &work, CoverClass operation, QByteArray &out)
    {
        const qsizetype slash = operation == CoverClass::Atmosphere ? -1 : work->id.indexOf(QLatin1Char('/'));
        const unsigned size = operation == CoverClass::Atmosphere ? 0 : work->id.left(slash).toUInt();
        const QString rest = operation == CoverClass::Atmosphere ? work->id : work->id.mid(slash + 1);
        QUrl url(rest);
        if (!url.isLocalFile())
            url = QUrl(QUrl::fromPercentEncoding(rest.toUtf8()));
        const QByteArray path = url.toLocalFile().toUtf8();
        if (operation == CoverClass::Atmosphere)
            return chilpa_atmosphere(path.constData(), out.data(), out.size());
        if (operation == CoverClass::Contained)
            return chilpa_contained_cover(path.constData(), size, out.data(), out.size());
        return slash > 0 && size > 0 && chilpa_cover(path.constData(), size, out.data(), out.size());
    }

    void complete(const std::shared_ptr<CoverWork> &work, const QString &coverPath, const QString &containedPath)
    {
        std::vector<CoverWork::Subscriber> subscribers;
        {
            QMutexLocker lock(&m_mutex);
            m_byKey.remove(work->key);
            m_running.erase(std::find(m_running.begin(), m_running.end(), work));
            subscribers = std::move(work->subscribers);
            dispatchLocked();
        }
        const bool coverWanted = std::any_of(subscribers.begin(), subscribers.end(), [](const auto &entry) {
            return entry.kind == CoverClass::Cover && !entry.response->cancelled();
        });
        const bool containedWanted = std::any_of(subscribers.begin(), subscribers.end(), [](const auto &entry) {
            return entry.kind == CoverClass::Contained && !entry.response->cancelled();
        });
        const bool atmosphereWanted = std::any_of(subscribers.begin(), subscribers.end(), [](const auto &entry) {
            return entry.kind == CoverClass::Atmosphere && !entry.response->cancelled();
        });
        const QImage coverImage = (coverWanted || atmosphereWanted) && !coverPath.isEmpty() ? QImage(coverPath) : QImage();
        const QImage containedImage = containedWanted && !containedPath.isEmpty() ? QImage(containedPath) : QImage();
        for (const auto &entry : subscribers) {
            CoverResponse *response = entry.response;
            const QImage &image = entry.kind == CoverClass::Contained ? containedImage : coverImage;
            if (response->cancelled())
                response->finish({}, QStringLiteral("cover request cancelled"));
            else if (image.isNull())
                response->finish({}, QStringLiteral("no cover"));
            else
                response->finish(image);
        }
    }

    void dispatchLocked()
    {
        while (m_running.size() < 2 && !m_pending.empty()) {
            const auto selected = selectLocked();
            auto work = *selected;
            m_pending.erase(selected);
            m_running.push_back(work);
            ++m_dispatches;
            m_pool.start(new CoverJob(shared_from_this(), work));
        }
    }

    std::vector<std::shared_ptr<CoverWork>>::iterator selectLocked()
    {
        const bool nonCoverRunning = std::any_of(m_running.begin(), m_running.end(), [](const auto &work) {
            return work->kind != CoverClass::Cover;
        });
        const bool coverPending = std::any_of(m_pending.begin(), m_pending.end(), [](const auto &work) {
            return work->kind == CoverClass::Cover;
        });
        for (int offset = 0; offset < 3; ++offset) {
            const int candidateClass = nonCoverRunning && coverPending ? 0 : (m_nextClass + offset) % 3;
            auto newest = m_pending.end();
            auto oldest = m_pending.end();
            for (auto current = m_pending.begin(); current != m_pending.end(); ++current) {
                if (static_cast<int>((*current)->kind) != candidateClass)
                    continue;
                if (newest == m_pending.end() || (*current)->sequence > (*newest)->sequence)
                    newest = current;
                if (oldest == m_pending.end() || (*current)->sequence < (*oldest)->sequence)
                    oldest = current;
            }
            if (newest == m_pending.end())
                continue;
            if (!(nonCoverRunning && coverPending))
                m_nextClass = (candidateClass + 1) % 3;
            if (m_dispatches - (*oldest)->dispatchAtEnqueue >= AgingDispatches)
                return oldest;
            return newest;
        }
        return m_pending.begin();
    }

    mutable QMutex m_mutex;
    QThreadPool m_pool;
    QHash<QString, std::shared_ptr<CoverWork>> m_byKey;
    std::vector<std::shared_ptr<CoverWork>> m_pending;
    std::vector<std::shared_ptr<CoverWork>> m_running;
    quint64 m_sequence = 0;
    quint64 m_dispatches = 0;
    int m_nextClass = 0;
};

void CoverJob::run() { m_scheduler->execute(m_work); }

void CoverResponse::cancel()
{
    if (m_cancelled.exchange(true, std::memory_order_acq_rel))
        return;
    if (const auto scheduler = m_scheduler.lock())
        scheduler->cancel(this);
    else
        finish({}, QStringLiteral("cover request cancelled"));
}

class CoverProvider : public QQuickAsyncImageProvider
{
public:
    CoverProvider(std::shared_ptr<CoverScheduler> scheduler, CoverClass kind = CoverClass::Cover)
        : m_scheduler(std::move(scheduler)), m_kind(kind) {}

    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &) override
    {
        return m_scheduler->request(m_kind, id);
    }

private:
    std::shared_ptr<CoverScheduler> m_scheduler;
    CoverClass m_kind;
};

} // namespace

extern "C" void chilpa_install_covers(void *engine)
{
    static NetworkFactory network;
    // Jobs may outlive their providers during engine teardown. Keep the shared
    // pool alive until process shutdown so it is never destroyed by a worker.
    static auto scheduler = std::make_shared<CoverScheduler>();
    static_cast<QQmlEngine *>(engine)->setNetworkAccessManagerFactory(&network);
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("cover"), new CoverProvider(scheduler));
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("contained"), new CoverProvider(scheduler, CoverClass::Contained));
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("atmosphere"), new CoverProvider(scheduler, CoverClass::Atmosphere));
}
