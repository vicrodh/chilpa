#include <QtCore/QResource>
#include <QtCore/QCoreApplication>
#include <QtCore/QByteArray>
#include <QtCore/QString>

// I register the same resource paths the application uses before the QML
// runner starts, while its bridge objects still come from test fixtures.
__attribute__((constructor)) static void resources()
{
    if (qEnvironmentVariableIsSet("CHILPA_QML_WEB_TEST"))
        QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QResource::registerResource(QString::fromUtf8(qgetenv("CHILPA_QML_RESOURCES")));
}
