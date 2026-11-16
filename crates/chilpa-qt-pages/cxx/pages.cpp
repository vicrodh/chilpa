#include <QtCore/QUrl>
#include <QtQml/qqml.h>

extern "C" void chilpa_pages_register_qml_types()
{
    qmlRegisterType(QUrl(QStringLiteral("qrc:/qt/qml/app/chilpa/pages/qml/NativeSourceHost.qml")),
        "app.chilpa.pages", 1, 0, "NativeSourceHost");
}
