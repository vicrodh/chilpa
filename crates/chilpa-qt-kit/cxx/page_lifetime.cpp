#include "page_lifetime.h"
#include <QtCore/QCoreApplication>
#include <QtCore/QPointer>
#include <QtCore/QSet>
#include <QtCore/QVariant>
#include <QtQml/qqml.h>
#include <QtQuick/QQuickItem>

namespace {
void cancelTree(QObject *root, QSet<QObject *> &visited)
{
    if (!root || visited.contains(root)) return;
    visited.insert(root);
    const QPointer<QObject> guard(root);
    QList<QPointer<QObject>> children;
    for (QObject *child : root->children()) children.append(child);
    if (auto *item = qobject_cast<QQuickItem *>(root))
        for (QQuickItem *child : item->childItems()) children.append(child);
    // Repeater delegates are visual siblings, with separate QObject owners.
    // Stop their loaders before clearing the model destroys their contexts.
    if (root->inherits("QQuickRepeater")) {
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
    if (root->inherits("QQuickLoader")) root->setProperty("active", false);
    else if (root->inherits("QQuickItemView") || root->inherits("QQuickRepeater"))
        root->setProperty("model", QVariant());
}
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
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_page_lifetime_register_qml_type)
