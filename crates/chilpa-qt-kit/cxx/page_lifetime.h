#ifndef CHILPA_PAGE_LIFETIME_H
#define CHILPA_PAGE_LIFETIME_H

#include <QtCore/QObject>

class PageLifetime : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;
    Q_INVOKABLE void cancel(QObject *root);
};

#endif
