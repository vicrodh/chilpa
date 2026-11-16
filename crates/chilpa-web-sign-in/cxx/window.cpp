// The sign-in window on Qt WebEngine: a page that ends at the plugin's return
// address. Which address ends it and which user agent the page sees are
// decided in Rust (src/native.rs and src/lib.rs); this file connects them to
// every place the engine shows an address.

#include <QApplication>
#include <QFileInfo>
#include <QLibraryInfo>
#include <QMetaObject>
#include <QUrl>
#include <QWebEngineNewWindowRequest>
#include <QWebEnginePage>
#include <QWebEnginePermission>
#include <QWebEngineProfile>
#include <QWebEngineProfileBuilder>
#include <QWebEngineSettings>
#include <QWebEngineUrlRequestInfo>
#include <QWebEngineUrlRequestInterceptor>
#include <QWebEngineView>

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <memory>

#if QT_VERSION < QT_VERSION_CHECK(6, 9, 0)
#error "the sign-in window needs Qt WebEngine 6.9 or later (QWebEngineProfileBuilder)"
#endif

// A borrowed UTF-8 string; empty means absent. The layout of `native::Text`.
struct ChilpaSignInText {
    const char *data;
    size_t size;
};

// The layout of `native::Window`.
struct ChilpaSignInWindow {
    ChilpaSignInText url;
    ChilpaSignInText title;
    ChilpaSignInText user_agent;
    ChilpaSignInText data_dir;
    ChilpaSignInText cache_dir;
    const void *session;
    bool (*ends)(const void *session, uint8_t seen, const char *address, size_t size);
    bool (*engine_token)(const char *agent, size_t size, size_t *start, size_t *length);
    void (*reached)(const void *session, const char *address, size_t size);
};

namespace {

// Where an address was seen: the codes of `Seen::from_code`.
enum class Seen : uint8_t { MainFrame = 0, SubFrame = 1, NewWindow = 2, Loaded = 3, Resource = 4 };

// The statuses `native::outcome` reads.
enum class Ending : uint8_t { Reached = 0, Closed = 1, Invalid = 2, Unavailable = 3 };

QString text(const ChilpaSignInText &value)
{
    return QString::fromUtf8(value.data, qsizetype(value.size));
}

// The first address the rules accept ends the sign-in: it goes to Rust and
// the window closes. Qt WebEngine calls every handler here on the window's
// thread, the request interceptor included (InterceptOnUIThread in its
// proxying_url_loader_factory_qt.cpp); the window closes from the event
// loop, after the engine's call returns.
class Finish {
public:
    explicit Finish(const ChilpaSignInWindow &request) : request(request) {}

    // Whether the address ends the sign-in. The first one that does is kept.
    bool take(Seen seen, const QUrl &url)
    {
        const QByteArray address = url.toEncoded();
        if (!request.ends(request.session, uint8_t(seen), address.constData(), size_t(address.size())))
            return false;
        if (!taken) {
            taken = true;
            request.reached(request.session, address.constData(), size_t(address.size()));
            QMetaObject::invokeMethod(window, [view = window] {
                view->page()->triggerAction(QWebEnginePage::Stop);
                view->close();
                QCoreApplication::exit(0);
            }, Qt::QueuedConnection);
        }
        return true;
    }

    const ChilpaSignInWindow &request;
    QWebEngineView *window = nullptr;
    bool taken = false;
};

class Page final : public QWebEnginePage {
public:
    Page(QWebEngineProfile *profile, Finish &finish, QObject *parent) : QWebEnginePage(profile, parent), finish(finish) {}

protected:
    // Every navigation of the page and its frames, server redirects included
    // (NavigationTypeRedirect).
    bool acceptNavigationRequest(const QUrl &url, NavigationType, bool isMainFrame) override
    {
        return !finish.take(isMainFrame ? Seen::MainFrame : Seen::SubFrame, url);
    }

    // A page's new windows open nowhere; newWindowRequested reads where they
    // were going.
    QWebEnginePage *createWindow(WebWindowType) override { return nullptr; }

private:
    Finish &finish;
};

// Every request the profile makes, so a redirect to the return address is
// refused even where no navigation decision was asked for.
class Interceptor final : public QWebEngineUrlRequestInterceptor {
public:
    explicit Interceptor(Finish &finish) : finish(finish) {}

    void interceptRequest(QWebEngineUrlRequestInfo &info) override
    {
        const auto type = info.resourceType();
        const Seen seen = type == QWebEngineUrlRequestInfo::ResourceTypeMainFrame ? Seen::MainFrame
                : type == QWebEngineUrlRequestInfo::ResourceTypeSubFrame          ? Seen::SubFrame
                                                                                   : Seen::Resource;
        if (finish.take(seen, info.requestUrl()))
            info.block(true);
    }

private:
    Finish &finish;
};

// Whether the engine's helper process is where Qt WebEngine looks for it
// (subProcessPath in its web_engine_library_info.cpp): only the variable when
// it is set, otherwise Qt's two folders and then this executable's. Without
// it Qt aborts, so a missing engine is told here instead. On macOS and
// Windows Qt looks elsewhere and decides by itself.
bool engineInstalled()
{
#if defined(Q_OS_MACOS) || defined(Q_OS_WIN)
    return true;
#else
    const QString chosen = qEnvironmentVariable("QTWEBENGINEPROCESS_PATH");
    if (!chosen.isEmpty())
        return QFileInfo::exists(chosen);
    const QString name = QStringLiteral("/QtWebEngineProcess");
    return QFileInfo::exists(QLibraryInfo::path(QLibraryInfo::LibraryExecutablesPath) + name)
            || QFileInfo::exists(QLibraryInfo::path(QLibraryInfo::BinariesPath) + name)
            || QFileInfo::exists(QCoreApplication::applicationDirPath() + name);
#endif
}

// A profile that keeps cookies, storage and cache in the folders Chilpa
// gives, or one that keeps nothing without them.
std::unique_ptr<QWebEngineProfile> profileFor(const ChilpaSignInWindow &request)
{
    if (request.data_dir.size == 0 || request.cache_dir.size == 0)
        return std::unique_ptr<QWebEngineProfile>(QWebEngineProfileBuilder::createOffTheRecordProfile());
    // The builder gives the folders before the profile opens anything; a
    // profile moved after it is made has already set up Qt's default folder.
    QWebEngineProfileBuilder builder;
    builder.setPersistentStoragePath(text(request.data_dir))
            .setCachePath(text(request.cache_dir))
            .setHttpCacheType(QWebEngineProfile::DiskHttpCache)
            .setPersistentCookiesPolicy(QWebEngineProfile::AllowPersistentCookies);
    return std::unique_ptr<QWebEngineProfile>(builder.createProfile(QStringLiteral("sign-in")));
}

// The declared user agent, or the engine's own without its Qt token when the
// rest is the Chromium one (`engine_token`).
void chooseUserAgent(QWebEngineProfile &profile, const ChilpaSignInWindow &request)
{
    if (request.user_agent.size != 0) {
        profile.setHttpUserAgent(text(request.user_agent));
    } else {
        QByteArray agent = profile.httpUserAgent().toUtf8();
        size_t start = 0;
        size_t length = 0;
        if (request.engine_token(agent.constData(), size_t(agent.size()), &start, &length)) {
            agent.remove(qsizetype(start), qsizetype(length));
            profile.setHttpUserAgent(QString::fromUtf8(agent));
        }
    }
    // stderr is not read by Chilpa; it says what a provider saw when the
    // window is run by hand.
    std::fprintf(stderr, "chilpa-web-sign-in: user agent %s\n", profile.httpUserAgent().toUtf8().constData());
}

} // namespace

extern "C" uint8_t chilpa_sign_in_window(const ChilpaSignInWindow *request)
{
    static int argc = 1;
    static char name[] = "chilpa-web-sign-in";
    static char *argv[] = {name, nullptr};
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QGuiApplication::setDesktopFileName(QStringLiteral("app.chilpa.Chilpa"));
    QApplication application(argc, argv);
    if (!engineInstalled())
        return uint8_t(Ending::Unavailable);
    const QUrl url(text(request->url), QUrl::StrictMode);
    if (!url.isValid() || url.scheme() != QStringLiteral("https"))
        return uint8_t(Ending::Invalid);

    // Declared in this order so they are destroyed in the other: the page
    // before its profile, the profile before its interceptor.
    Finish finish(*request);
    Interceptor interceptor(finish);
    const std::unique_ptr<QWebEngineProfile> profile = profileFor(*request);
    profile->setUrlRequestInterceptor(&interceptor);
    chooseUserAgent(*profile, *request);
    QWebEngineSettings *settings = profile->settings();
    settings->setAttribute(QWebEngineSettings::PlaybackRequiresUserGesture, true);
    settings->setAttribute(QWebEngineSettings::ScreenCaptureEnabled, false);
    settings->setAttribute(QWebEngineSettings::WebRTCPublicInterfacesOnly, true);
    settings->setAttribute(QWebEngineSettings::PluginsEnabled, false);

    QWebEngineView view;
    auto *page = new Page(profile.get(), finish, &view);
    view.setPage(page);
    finish.window = &view;
    page->setAudioMuted(true);
    // Camera, microphone, screen, location, notifications: a sign-in needs none.
    QObject::connect(page, &QWebEnginePage::permissionRequested, page, [](QWebEnginePermission permission) { permission.deny(); });
    QObject::connect(page, &QWebEnginePage::newWindowRequested, page,
            [&finish](QWebEngineNewWindowRequest &window) { finish.take(Seen::NewWindow, window.requestedUrl()); });
    // A page that reaches the address without a navigation decision, such as
    // one restored from history, is caught when its address changes or it
    // finishes loading.
    QObject::connect(page, &QWebEnginePage::urlChanged, page, [&finish](const QUrl &address) { finish.take(Seen::Loaded, address); });
    QObject::connect(page, &QWebEnginePage::loadFinished, page, [&finish, page](bool) { finish.take(Seen::Loaded, page->url()); });

    view.setWindowTitle(text(request->title));
    view.resize(560, 760);
    view.show();
    page->load(url);
    application.exec();
    return uint8_t(finish.taken ? Ending::Reached : Ending::Closed);
}
