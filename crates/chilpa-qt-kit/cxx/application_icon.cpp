// I leave compiled macOS icons to the system and adapt unbundled builds too.
#include <QtCore/QFile>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QStyleHints>

extern "C" void chilpa_install_application_icon()
{
#ifdef Q_OS_MACOS
    const auto resources = QCoreApplication::applicationDirPath() + QStringLiteral("/../Resources/");
    if (QFile::exists(resources + QStringLiteral("Assets.car")))
        return;
    const auto refresh = [] {
        const bool dark = QGuiApplication::styleHints()->colorScheme() == Qt::ColorScheme::Dark;
        QGuiApplication::setWindowIcon(QIcon(dark
            ? QStringLiteral(":/qt/qml/app/chilpa/kit/assets/app-icon-dark.png")
            : QStringLiteral(":/qt/qml/app/chilpa/kit/assets/app-icon-light.png")));
    };
    refresh();
    QObject::connect(QGuiApplication::styleHints(), &QStyleHints::colorSchemeChanged, qGuiApp, refresh);
#else
    QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/qt/qml/app/chilpa/kit/assets/logo.png")));
#endif
}
