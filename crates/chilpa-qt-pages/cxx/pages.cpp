#include <QtCore/QCoreApplication>
#include <QtCore/QUrl>
#include <QtQml/qqml.h>

extern "C" void chilpa_prepare_embedded_pages()
{
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    qmlRegisterType(QUrl(QStringLiteral("qrc:/qt/qml/app/chilpa/pages/qml/NativeSourceHost.qml")),
        "app.chilpa.pages", 1, 0, "NativeSourceHost");
}
