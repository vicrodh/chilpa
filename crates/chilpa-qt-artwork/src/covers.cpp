// The window's covers, at the size they are drawn (frontend ADR-0014,
// rule 5): an image provider behind `image://cover/<size>/<path>`.
//
// All three providers share two workers. Within its class, pending work is
// served oldest first: a list asks for its visible rows before its buffer
// rows, so at rest the screen fills top-down before anything off screen. A
// pending request withdrawn while others still wait means rows are leaving
// the screen; until the queue drains, the class then alternates its newest
// and oldest requests, so the leading edge of a scroll gets every other
// dispatch and the rows already on screen the rest. A jump withdraws the old
// screen before the new one asks, so the queue drains and the new screen is
// served oldest first again. Classes take round-robin turns so contained
// covers and atmosphere cannot starve row covers or be starved by them. When
// covers are pending, non-cover work occupies at most one worker so headers
// and immersive views cannot take both cover slots. Pending producers exist
// only while at least one live Image is subscribed, so demand bounds the
// queue without rejecting it.

#include <QtCore/QHash>
#include <QtCore/QMetaObject>
#include <QtCore/QMutex>
#include <QtCore/QThread>
#include <QtCore/QThreadPool>
#include <QtCore/QUrl>
#include <QtGui/QImage>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlEngine>
#include <QtQml/QQmlNetworkAccessManagerFactory>
#include <QtQuick/QQuickImageProvider>
#include <algorithm>
#include <array>
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
enum class WorkOperation { Derivative, Atmosphere };
enum class WorkPhase { Base, Contained };
class CoverScheduler;

#ifdef CHILPA_COVER_SCHEDULER_TEST
void coverTestBeforeDelivery();
void coverTestResponseCreated();
void coverTestResponseDestroyed();
void coverTestResponseFinished();
void coverTestWorkCompleted();
void coverTestBeforeImageLoad();
void coverTestAfterSplitRegistered();
void coverTestDeliveryQueued();
void coverTestTsanNegativeCallback();
void coverTestTsanWorkerNegative();
#endif

class CoverResponse : public QQuickImageResponse
{
public:
    explicit CoverResponse(std::weak_ptr<CoverScheduler> scheduler) : m_scheduler(std::move(scheduler))
    {
        QObject::connect(thread(), &QThread::finished, this, [this] { shutdown(); }, Qt::DirectConnection);
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestResponseCreated();
#endif
    }
    ~CoverResponse() override
    {
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestResponseDestroyed();
#endif
    }

    void cancel() override;
    QQuickTextureFactory *textureFactory() const override { return QQuickTextureFactory::textureFactoryForImage(m_image); }
    QString errorString() const override { return m_error; }
    bool cancelled() const { return m_cancelled.load(std::memory_order_acquire); }

    void finish(QImage image = {}, QString error = {})
    {
        {
            QMutexLocker lock(&m_completionMutex);
            if (m_completionScheduled)
                return;
            m_completionScheduled = true;
            m_pendingImage = std::move(image);
            m_pendingError = std::move(error);
        }
        {
            QMutexLocker handoff(&m_deliveryHandoffMutex);
            QMetaObject::invokeMethod(this, &CoverResponse::deliverPending, Qt::QueuedConnection);
        }
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestDeliveryQueued();
#endif
    }

private:
    void shutdown();

    void deliverPending()
    {
        {
            QMutexLocker handoff(&m_deliveryHandoffMutex);
        }
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestTsanNegativeCallback();
#endif
        QImage image;
        QString error;
        {
            QMutexLocker lock(&m_completionMutex);
            image = std::move(m_pendingImage);
            error = std::move(m_pendingError);
        }
        deliver(std::move(image), std::move(error));
    }

    void deliver(QImage image, QString error)
    {
        if (m_finished.exchange(true, std::memory_order_acq_rel))
            return;
        m_image = std::move(image);
        m_error = std::move(error);
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestResponseFinished();
#endif
        emit finished();
    }

    std::weak_ptr<CoverScheduler> m_scheduler;
    QImage m_image;
    QString m_error;
    QMutex m_completionMutex;
    QMutex m_deliveryHandoffMutex;
    QImage m_pendingImage;
    QString m_pendingError;
    std::atomic_bool m_cancelled{false};
    bool m_completionScheduled = false;
    std::atomic_bool m_finished{false};
};

struct CoverWork {
    struct Subscriber { CoverResponse *response; CoverClass kind; };
    QString key;
    QString id;
    WorkOperation operation;
    CoverClass priority;
    WorkPhase phase = WorkPhase::Base;
    QString coverPath;
    quint64 firstSeen = 0;
    quint64 lastDemand = 0;
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
    CoverScheduler() { m_pool.setMaxThreadCount(2); }
    ~CoverScheduler() { m_pool.waitForDone(); }

    CoverResponse *request(CoverClass kind, const QString &id)
    {
        auto *response = new CoverResponse(weak_from_this());
        std::shared_ptr<CoverWork> readyCover;
        QString readyCoverPath;
        {
            QMutexLocker lock(&m_mutex);
            QString key = kind == CoverClass::Atmosphere ? QStringLiteral("atmosphere:") + id
                                                         : QStringLiteral("derivative:") + id;
            if (const auto existing = m_byKey.value(key)) {
                if (kind == CoverClass::Cover && existing->phase == WorkPhase::Contained) {
                    readyCover = std::make_shared<CoverWork>();
                    readyCover->id = id;
                    readyCover->subscribers.push_back({response, kind});
                    readyCoverPath = existing->coverPath;
                    m_delivering.push_back(readyCover);
                } else {
                    existing->subscribers.push_back({response, kind});
                    existing->lastDemand = ++m_demandSequence;
                    if (kind == CoverClass::Cover)
                        existing->priority = CoverClass::Cover;
                }
            } else {
                auto work = std::make_shared<CoverWork>();
                work->key = key;
                work->id = id;
                work->operation = kind == CoverClass::Atmosphere ? WorkOperation::Atmosphere
                                                                 : WorkOperation::Derivative;
                work->priority = kind;
                work->firstSeen = work->lastDemand = ++m_demandSequence;
                work->subscribers.push_back({response, kind});
                m_pending.push_back(work);
                m_byKey.insert(key, work);
                dispatchLocked();
            }
        }
        if (readyCover)
            deliverClosed(readyCover, readyCoverPath, {}, true, false, false);
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
                if (m_pending.empty())
                    drainedLocked();
                else
                    m_scrolling = true;
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
            if (!found) {
                for (const auto &work : std::as_const(m_delivering)) {
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
        Q_UNUSED(found);
    }

    void execute(const std::shared_ptr<CoverWork> &work)
    {
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestTsanWorkerNegative();
#endif
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
        if (work->operation == WorkOperation::Atmosphere) {
            const bool made = make(work, CoverClass::Atmosphere, coverOut);
            complete(work, made ? QString::fromUtf8(coverOut.constData()) : QString(), {});
            return;
        }
        // A contained cover takes two queue turns: the base derivative, which
        // plain cover askers of the same key share, then the contained
        // picture made from it.
        if (work->phase == WorkPhase::Contained) {
            const bool made = make(work, CoverClass::Contained, containedOut);
            complete(work, work->coverPath, made ? QString::fromUtf8(containedOut.constData()) : QString());
            return;
        }
        const bool coverMade = make(work, CoverClass::Cover, coverOut);
        continueAfterBase(work, coverMade ? QString::fromUtf8(coverOut.constData()) : QString());
    }

#ifdef CHILPA_COVER_SCHEDULER_TEST
    qsizetype pendingCount() const
    {
        QMutexLocker lock(&m_mutex);
        return m_pending.size();
    }

    bool idle() const
    {
        QMutexLocker lock(&m_mutex);
        return m_pending.empty() && m_running.empty() && m_delivering.empty()
            && m_pool.activeThreadCount() == 0;
    }
#endif

private:
    void continueAfterBase(const std::shared_ptr<CoverWork> &work, QString coverPath)
    {
        bool coverWanted = false;
        bool containedWanted = false;
        bool atmosphereWanted = false;
        std::shared_ptr<CoverWork> coverDelivery;
        bool coverDeliveryRegistered = false;
        {
            QMutexLocker lock(&m_mutex);
            const bool makeContained = std::any_of(work->subscribers.begin(), work->subscribers.end(), [](const auto &entry) {
                return entry.kind == CoverClass::Contained;
            });
            if (makeContained) {
                coverDelivery = std::make_shared<CoverWork>();
                coverDelivery->id = work->id;
                auto subscriber = work->subscribers.begin();
                while (subscriber != work->subscribers.end()) {
                    if (subscriber->kind == CoverClass::Cover) {
                        coverDelivery->subscribers.push_back(*subscriber);
                        subscriber = work->subscribers.erase(subscriber);
                    } else {
                        ++subscriber;
                    }
                }
                work->coverPath = coverPath;
                work->phase = WorkPhase::Contained;
                work->priority = CoverClass::Contained;
                m_running.erase(std::find(m_running.begin(), m_running.end(), work));
                m_pending.push_back(work);
                if (!coverDelivery->subscribers.empty()) {
                    m_delivering.push_back(coverDelivery);
                    coverDeliveryRegistered = true;
                }
                dispatchLocked();
            } else {
                closeForDeliveryLocked(work, coverWanted, containedWanted, atmosphereWanted);
            }
        }
#ifdef CHILPA_COVER_SCHEDULER_TEST
        if (coverDeliveryRegistered)
            coverTestAfterSplitRegistered();
#endif
        if (coverDelivery) {
            if (coverDeliveryRegistered)
                deliverClosed(coverDelivery, coverPath, {}, true, false, false);
            return;
        }
        deliverClosed(work, coverPath, {}, coverWanted, containedWanted, atmosphereWanted);
    }

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
        bool coverWanted = false;
        bool containedWanted = false;
        bool atmosphereWanted = false;
        {
            QMutexLocker lock(&m_mutex);
            closeForDeliveryLocked(work, coverWanted, containedWanted, atmosphereWanted);
        }
        deliverClosed(work, coverPath, containedPath, coverWanted, containedWanted, atmosphereWanted);
    }

    void closeForDeliveryLocked(const std::shared_ptr<CoverWork> &work, bool &coverWanted,
                                bool &containedWanted, bool &atmosphereWanted)
    {
        for (const auto &entry : work->subscribers) {
            coverWanted |= entry.kind == CoverClass::Cover;
            containedWanted |= entry.kind == CoverClass::Contained;
            atmosphereWanted |= entry.kind == CoverClass::Atmosphere;
        }
        m_byKey.remove(work->key);
        m_running.erase(std::find(m_running.begin(), m_running.end(), work));
        m_delivering.push_back(work);
        dispatchLocked();
    }

    void deliverClosed(const std::shared_ptr<CoverWork> &work, const QString &coverPath,
                       const QString &containedPath, bool coverWanted, bool containedWanted,
                       bool atmosphereWanted)
    {
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestBeforeImageLoad();
#endif
        const CoverClass coverOperation = atmosphereWanted ? CoverClass::Atmosphere : CoverClass::Cover;
        const QImage coverImage = (coverWanted || atmosphereWanted) && !coverPath.isEmpty()
            ? load(work, coverOperation, coverPath) : QImage();
        const QImage containedImage = containedWanted && !containedPath.isEmpty()
            ? load(work, CoverClass::Contained, containedPath) : QImage();
#ifdef CHILPA_COVER_SCHEDULER_TEST
        coverTestBeforeDelivery();
#endif
        {
            QMutexLocker lock(&m_mutex);
            for (const auto &entry : work->subscribers) {
                CoverResponse *response = entry.response;
                const QImage &image = entry.kind == CoverClass::Contained ? containedImage : coverImage;
                if (response->cancelled())
                    response->finish({}, QStringLiteral("cover request cancelled"));
                else if (image.isNull())
                    response->finish({}, QStringLiteral("no cover"));
                else
                    response->finish(image);
            }
            work->subscribers.clear();
            m_delivering.erase(std::find(m_delivering.begin(), m_delivering.end(), work));
        }
    }

    // The cache sweep can remove a file between the facade naming it and
    // this read. The facade is asked once more, which makes the file again;
    // a second failure is delivered as an error.
    QImage load(const std::shared_ptr<CoverWork> &work, CoverClass operation, const QString &path)
    {
        QImage image(path);
        if (!image.isNull())
            return image;
        QByteArray out(4096, '\0');
        return make(work, operation, out) ? QImage(QString::fromUtf8(out.constData())) : QImage();
    }

    void dispatchLocked()
    {
        while (m_running.size() < 2 && !m_pending.empty()) {
            const auto selected = selectLocked();
            auto work = *selected;
            m_pending.erase(selected);
            if (m_pending.empty())
                drainedLocked();
            m_running.push_back(work);
            m_pool.start(new CoverJob(shared_from_this(), work));
        }
    }

    CoverClass runningClassLocked(const std::shared_ptr<CoverWork> &work) const
    {
        if (work->operation == WorkOperation::Atmosphere)
            return CoverClass::Atmosphere;
        if (work->phase == WorkPhase::Contained)
            return CoverClass::Contained;
        return CoverClass::Cover;
    }

    std::vector<std::shared_ptr<CoverWork>>::iterator selectLocked()
    {
        const bool nonCoverRunning = std::any_of(m_running.begin(), m_running.end(), [this](const auto &work) {
            return runningClassLocked(work) != CoverClass::Cover;
        });
        const bool coverPending = std::any_of(m_pending.begin(), m_pending.end(), [](const auto &work) {
            return work->priority == CoverClass::Cover;
        });
        for (int offset = 0; offset < 3; ++offset) {
            const int candidateClass = nonCoverRunning && coverPending ? 0 : (m_nextClass + offset) % 3;
            auto newest = m_pending.end();
            auto oldest = m_pending.end();
            for (auto current = m_pending.begin(); current != m_pending.end(); ++current) {
                if (static_cast<int>((*current)->priority) != candidateClass)
                    continue;
                if (newest == m_pending.end() || (*current)->lastDemand > (*newest)->lastDemand)
                    newest = current;
                if (oldest == m_pending.end() || (*current)->firstSeen < (*oldest)->firstSeen)
                    oldest = current;
            }
            if (newest == m_pending.end())
                continue;
            if (!(nonCoverRunning && coverPending))
                m_nextClass = (candidateClass + 1) % 3;
            if (!m_scrolling)
                return oldest;
            const bool oldestTurn = ++m_classDispatches[candidateClass] % 2 == 0;
            return oldestTurn ? oldest : newest;
        }
        return m_pending.begin();
    }

    void drainedLocked()
    {
        m_scrolling = false;
        m_classDispatches = {};
    }

    mutable QMutex m_mutex;
    QThreadPool m_pool;
    QHash<QString, std::shared_ptr<CoverWork>> m_byKey;
    std::vector<std::shared_ptr<CoverWork>> m_pending;
    std::vector<std::shared_ptr<CoverWork>> m_running;
    std::vector<std::shared_ptr<CoverWork>> m_delivering;
    quint64 m_demandSequence = 0;
    // A pending request was withdrawn since the queue last drained.
    bool m_scrolling = false;
    std::array<quint64, 3> m_classDispatches{};
    int m_nextClass = 0;
};

void CoverJob::run()
{
    m_scheduler->execute(m_work);
#ifdef CHILPA_COVER_SCHEDULER_TEST
    coverTestWorkCompleted();
#endif
}

void CoverResponse::cancel()
{
    if (m_cancelled.exchange(true, std::memory_order_acq_rel))
        return;
    if (const auto scheduler = m_scheduler.lock())
        scheduler->cancel(this);
    finish({}, QStringLiteral("cover request cancelled"));
}

void CoverResponse::shutdown()
{
    m_cancelled.store(true, std::memory_order_release);
    if (const auto scheduler = m_scheduler.lock())
        scheduler->cancel(this);
    deliver({}, QStringLiteral("cover request cancelled"));
    delete this;
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
