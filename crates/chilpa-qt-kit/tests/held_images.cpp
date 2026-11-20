#include <QtCore/QCoreApplication>
#include <QtCore/QHash>
#include <QtCore/QList>
#include <QtCore/QMutex>
#include <QtCore/QSet>
#include <QtGui/QColor>
#include <QtGui/QImage>
#include <QtQml/QQmlEngine>
#include <QtQml/QQmlPropertyMap>
#include <QtQml/qqml.h>
#include <QtQuick/QQuickImageProvider>

// Pictures that arrive when a test says so. `image://held/<name>/<color>` stays loading until the test writes
// <name>, or "*" for all, into `HeldImages.open`; then it arrives as a square of that color. Importing
// app.chilpa.testing and reading HeldImages once adds the provider to the test's engine.
namespace {

class HeldResponse : public QQuickImageResponse
{
public:
    explicit HeldResponse(const QString &color) : m_color(color) {}
    QQuickTextureFactory *textureFactory() const override
    {
        QImage image(16, 16, QImage::Format_ARGB32_Premultiplied);
        image.fill(QColor::fromString(m_color));
        return QQuickTextureFactory::textureFactoryForImage(image);
    }
    // Emitted once, on the reader's thread, after the reader has connected to it.
    void arrive() { QMetaObject::invokeMethod(this, [this] { Q_EMIT finished(); }, Qt::QueuedConnection); }

private:
    QString m_color;
};

struct Held {
    QMutex lock;
    QMultiHash<QString, HeldResponse *> waiting;
    QSet<QString> open;
};

Held &held()
{
    static Held state;
    return state;
}

// A response lives until it finishes, also when its picture is no longer wanted, so each held one is let go once.
void open(const QString &name)
{
    QList<HeldResponse *> going;
    {
        QMutexLocker guard(&held().lock);
        if (name == QLatin1String("*")) {
            going = held().waiting.values();
            held().waiting.clear();
        } else {
            held().open.insert(name);
            going = held().waiting.values(name);
            held().waiting.remove(name);
        }
    }
    for (HeldResponse *response : going) response->arrive();
}

class HeldProvider : public QQuickAsyncImageProvider
{
public:
    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &) override
    {
        const QString name = id.section(QLatin1Char('/'), 0, 0);
        auto *response = new HeldResponse(id.section(QLatin1Char('/'), 1));
        QMutexLocker guard(&held().lock);
        if (held().open.contains(name)) response->arrive();
        else held().waiting.insert(name, response);
        return response;
    }
};

void registerHeldImages()
{
    qmlRegisterSingletonType<QQmlPropertyMap>("app.chilpa.testing", 1, 0, "HeldImages", [](QQmlEngine *engine, QJSEngine *) {
        engine->addImageProvider(QStringLiteral("held"), new HeldProvider);
#if QT_VERSION >= QT_VERSION_CHECK(6, 11, 0)
        auto *map = QQmlPropertyMap::create();
#else
        auto *map = new QQmlPropertyMap;
#endif
        map->insert(QStringLiteral("open"), QString());
        QObject::connect(map, &QQmlPropertyMap::valueChanged, map, [](const QString &key, const QVariant &value) {
            if (key == QLatin1String("open") && !value.toString().isEmpty()) open(value.toString());
        });
        return map;
    });
}

}

Q_COREAPP_STARTUP_FUNCTION(registerHeldImages)
