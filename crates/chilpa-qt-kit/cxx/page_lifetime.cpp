#include "page_lifetime.h"
#include <QtCore/QCoreApplication>
#include <QtCore/QPointer>
#include <QtCore/QVariant>
#include <QtQml/qqml.h>

void PageLifetime::cancel(QObject *root)
{
    if (!root) return;
    const QPointer<QObject> guard(root);
    QList<QPointer<QObject>> children;
    for (QObject *child : root->children()) children.append(child);
    // Cancel from the leaves while their creation contexts are still valid.
    // Loader otherwise invalidates the entire context before deleteLater()
    // destroys its children, leaving their incubators runnable in between.
    for (const auto &child : children) {
        if (!guard) return;
        if (child) cancel(child);
    }
    if (!guard) return;
    if (root->inherits("QQuickLoader")) root->setProperty("active", false);
    else if (root->inherits("QQuickItemView") || root->inherits("QQuickRepeater"))
        root->setProperty("model", QVariant());
}

static void registerPageLifetime()
{
    qmlRegisterType<PageLifetime>("app.chilpa.kit", 1, 0, "PageLifetime");
}

Q_COREAPP_STARTUP_FUNCTION(registerPageLifetime)
