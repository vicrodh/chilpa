#ifndef CHILPA_PAGE_LIFETIME_H
#define CHILPA_PAGE_LIFETIME_H

#include <QtCore/QObject>
#include <QtQml/qqml.h>

class PageLifetime : public QObject
{
    Q_OBJECT
    Q_PROPERTY(bool ownMaterial READ ownsMaterial WRITE setOwnMaterial)
public:
    using QObject::QObject;
    static PageLifetime *qmlAttachedProperties(QObject *root);
    bool ownsMaterial() const { return materialRequested_; }
    void setOwnMaterial(bool requested);
    // Terminal retirement: cancel nested creation before replacing its
    // context. Resources removed by live models need ownership at creation.
    Q_INVOKABLE void cancel(QObject *root);
private:
    bool materialRequested_ = false;
};

QML_DECLARE_TYPEINFO(PageLifetime, QML_HAS_ATTACHED_PROPERTIES)

extern "C" void chilpa_page_lifetime_register_qml_type();

#endif
