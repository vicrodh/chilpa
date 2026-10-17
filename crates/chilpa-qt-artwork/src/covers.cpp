// The window's covers, at the size they are drawn (frontend ADR-0014,
// rule 5): an image provider behind `image://cover/<size>/<path>`.
//
// I share two workers between all three providers. Rust makes or finds the
// derivative on disk (`chilpa_cover`); this only loads the small file it names.

#include <QtCore/QThreadPool>
#include <QtCore/QUrl>
#include <QtGui/QImage>
#include <QtQml/QQmlEngine>
#include <QtQuick/QQuickImageProvider>
#include <QtNetwork/QNetworkAccessManager>
#include <QtNetwork/QNetworkRequest>
#include <QtQml/QQmlNetworkAccessManagerFactory>
#include <atomic>
#include <memory>

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

class CoverResponse : public QQuickImageResponse, public QRunnable
{
public:
    CoverResponse(const QString &id, int atmosphere) : m_id(id), m_atmosphere(atmosphere) { setAutoDelete(false); }

    void run() override
    {
        if (m_cancelled.load(std::memory_order_relaxed)) {
            emit finished();
            return;
        }
        const qsizetype slash = m_atmosphere == 1 ? -1 : m_id.indexOf(QLatin1Char('/'));
        const unsigned size = m_atmosphere == 1 ? 0 : m_id.left(slash).toUInt();
        const QString rest = m_atmosphere == 1 ? m_id : m_id.mid(slash + 1);
        QUrl url(rest);
        if (!url.isLocalFile())
            url = QUrl(QUrl::fromPercentEncoding(rest.toUtf8()));
        const QByteArray path = url.toLocalFile().toUtf8();
        QByteArray out(4096, '\0');
        const bool made = m_atmosphere == 1 ? chilpa_atmosphere(path.constData(), out.data(), out.size())
                                       : m_atmosphere == 2 ? chilpa_contained_cover(path.constData(), size, out.data(), out.size())
                                       : slash > 0 && size > 0 && chilpa_cover(path.constData(), size, out.data(), out.size());
        if (made && !m_cancelled.load(std::memory_order_relaxed))
            m_image = QImage(QString::fromUtf8(out.constData()));
        emit finished();
    }

    void cancel() override
    {
        // I leave completion to run(): Qt may delete me after finished().
        m_cancelled.store(true, std::memory_order_relaxed);
    }

    QQuickTextureFactory *textureFactory() const override
    {
        return QQuickTextureFactory::textureFactoryForImage(m_image);
    }

    QString errorString() const override
    {
        return m_image.isNull() ? QStringLiteral("no cover") : QString();
    }

private:
    QString m_id;
    int m_atmosphere;
    QImage m_image;
    std::atomic_bool m_cancelled{false};
};

class CoverProvider : public QQuickAsyncImageProvider
{
public:
    CoverProvider(std::shared_ptr<QThreadPool> pool, int atmosphere = 0)
        : m_pool(std::move(pool)), m_atmosphere(atmosphere) {}

    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &) override
    {
        auto *response = new CoverResponse(id, m_atmosphere);
        m_pool->start(response);
        return response;
    }

private:
    std::shared_ptr<QThreadPool> m_pool;
    int m_atmosphere;
};

} // namespace

extern "C" void chilpa_install_covers(void *engine)
{
    static NetworkFactory network;
    auto pool = std::make_shared<QThreadPool>();
    pool->setMaxThreadCount(2);
    static_cast<QQmlEngine *>(engine)->setNetworkAccessManagerFactory(&network);
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("cover"), new CoverProvider(pool));
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("contained"), new CoverProvider(pool, 2));
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("atmosphere"), new CoverProvider(pool, 1));
}
