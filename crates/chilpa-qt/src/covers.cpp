// The window's covers, at the size they are drawn (frontend ADR-0014,
// rule 5): an image provider behind `image://cover/<size>/<path>`.
//
// Each request runs on a pool of its own, not the global one: making a
// derivative of a big cover takes a fifth of a second and a hundred MB, and
// the global pool has a thread per core. Rust makes or finds the derivative
// on disk (`chilpa_cover`); this only loads the small file it names.

#include <QtCore/QThreadPool>
#include <QtCore/QUrl>
#include <QtGui/QImage>
#include <QtQml/QQmlEngine>
#include <QtQuick/QQuickImageProvider>

extern "C" bool chilpa_cover(const char *path, unsigned size, char *out, unsigned long capacity);
extern "C" bool chilpa_contained_cover(const char *path, unsigned size, char *out, unsigned long capacity);
extern "C" bool chilpa_atmosphere(const char *path, char *out, unsigned long capacity);

namespace {

class CoverResponse : public QQuickImageResponse, public QRunnable
{
public:
    CoverResponse(const QString &id, int atmosphere) : m_id(id), m_atmosphere(atmosphere) { setAutoDelete(false); }

    void run() override
    {
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
        if (made)
            m_image = QImage(QString::fromUtf8(out.constData()));
        emit finished();
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
};

class CoverProvider : public QQuickAsyncImageProvider
{
public:
    CoverProvider(int atmosphere = 0) : m_atmosphere(atmosphere) { m_pool.setMaxThreadCount(3); }

    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &) override
    {
        auto *response = new CoverResponse(id, m_atmosphere);
        m_pool.start(response);
        return response;
    }

private:
    QThreadPool m_pool;
    int m_atmosphere;
};

} // namespace

extern "C" void chilpa_install_covers(void *engine)
{
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("cover"), new CoverProvider);
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("contained"), new CoverProvider(2));
    static_cast<QQmlEngine *>(engine)->addImageProvider(QStringLiteral("atmosphere"), new CoverProvider(1));
}
