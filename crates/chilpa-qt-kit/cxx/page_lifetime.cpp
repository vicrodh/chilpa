#include "page_lifetime.h"
#include <QtCore/QCoreApplication>
#include <QtCore/QPointer>
#include <QtCore/QSet>
#include <QtCore/QUrl>
#include <QtCore/QVariant>
#include <QtQml/qqml.h>
#include <QtQml/QQmlProperty>
#include <QtQuick/QQuickItem>
#include <cstring>

namespace {
enum class Content { None, Loader, View, Repeater };

void ownShadowMaterial(QObject *root)
{
    if (!root || !root->inherits("QQuickRectangularShadow")) return;
    auto *material = root->property("material").value<QQuickItem *>();
    // A closed popup's default material has not joined a window and can
    // lack both parents. Custom materials already have a visual parent or
    // QML context; keep their ownership when retiring the shadow.
    if (material && !material->parent() && !qmlContext(material)
            && std::strcmp(material->metaObject()->className(), "QQuickShaderEffect") == 0
            && (!material->parentItem() || (material->parentItem() == root
                && material->property("vertexShader").toUrl()
                    == QUrl(QStringLiteral("qrc:/data/shaders/rectangularshadow.vert.qsb")))))
        material->setParent(root);
}

void cancelTree(QObject *root, QSet<QObject *> &visited)
{
    if (!root || visited.contains(root)) return;
    visited.insert(root);
    const QPointer<QObject> guard(root);
    ownShadowMaterial(root);
    bool visual = false;
    Content content = Content::None;
    // Read the class hierarchy once without invoking every generated metacast.
    for (const QMetaObject *type = root->metaObject(); type; type = type->superClass()) {
        if (type == &QQuickItem::staticMetaObject) { visual = true; break; }
        if (std::strcmp(type->className(), "QQuickLoader") == 0) content = Content::Loader;
        else if (std::strcmp(type->className(), "QQuickItemView") == 0) content = Content::View;
        else if (std::strcmp(type->className(), "QQuickRepeater") == 0) content = Content::Repeater;
    }
    QList<QPointer<QObject>> children;
    for (QObject *child : root->children()) children.append(child);
    if (visual)
        for (QQuickItem *child : static_cast<QQuickItem *>(root)->childItems()) children.append(child);
    // Repeater delegates are visual siblings, with separate QObject owners.
    // Stop their loaders before clearing the model destroys their contexts.
    if (content == Content::Repeater) {
        const int count = root->property("count").toInt();
        for (int index = 0; index < count; ++index) {
            QQuickItem *item = nullptr;
            QMetaObject::invokeMethod(root, "itemAt", Qt::DirectConnection,
                                      Q_RETURN_ARG(QQuickItem*, item), Q_ARG(int, index));
            if (item) children.append(item);
        }
    }
    // Cancel from the leaves while creation contexts are still valid.
    // Loader invalidates contexts before deleteLater() destroys children.
    for (const auto &child : children) {
        if (!guard) return;
        if (child) cancelTree(child, visited);
    }
    if (!guard) return;
    // A C++ setter leaves QML bindings attached; geometry changes could
    // otherwise reactivate a retired loader while its parent is clearing.
    if (content == Content::Loader) QQmlProperty::write(root, QStringLiteral("active"), false);
    else if (content == Content::View || content == Content::Repeater)
        QQmlProperty::write(root, QStringLiteral("model"), QVariant());
}
}

PageLifetime *PageLifetime::qmlAttachedProperties(QObject *root)
{
    auto *lifetime = new PageLifetime(root);
    lifetime->setOwnMaterial(true);
    return lifetime;
}

void PageLifetime::setOwnMaterial(bool requested)
{
    if (!requested || materialRequested_) return;
    ownShadowMaterial(parent());
    materialRequested_ = true;
}

void PageLifetime::cancel(QObject *root)
{
    QSet<QObject *> visited;
    cancelTree(root, visited);
}

extern "C" void chilpa_page_lifetime_register_qml_type()
{
    static bool registered = false;
    if (registered) return;
    registered = true;
    qmlRegisterType<PageLifetime>("app.chilpa.kit", 1, 0, "PageLifetime");
    // This primitive bootstrap uses Qt only; it exposes no facade state.
    qmlRegisterUncreatableType<PageLifetime>("app.chilpa.kit.primitives", 1, 0, "MaterialLifetime",
        "MaterialLifetime is attached by the shared Shadow primitive.");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_page_lifetime_register_qml_type)
