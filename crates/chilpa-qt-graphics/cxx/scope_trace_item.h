// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#ifndef CHILPA_SCOPE_TRACE_ITEM_H
#define CHILPA_SCOPE_TRACE_ITEM_H

#include <QtCore/QVariantList>
#include <QtCore/QVector>
#include <QtGui/QColor>
#include <QtQuick/QQuickItem>

class ScopeTraceItem : public QQuickItem
{
    Q_OBJECT
    Q_PROPERTY(QVariantList points READ points WRITE setPoints)
    Q_PROPERTY(QColor traceColor READ traceColor WRITE setTraceColor)
    Q_PROPERTY(int mode READ mode WRITE setMode)
    Q_PROPERTY(qreal lineWidth READ lineWidth WRITE setLineWidth)
    Q_PROPERTY(int trailDepth READ trailDepth WRITE setTrailDepth)
    Q_PROPERTY(qreal fillOpacity READ fillOpacity WRITE setFillOpacity)

public:
    explicit ScopeTraceItem(QQuickItem *parent = nullptr);

    QVariantList points() const { return m_points; }
    void setPoints(const QVariantList &points);
    QColor traceColor() const { return m_traceColor; }
    void setTraceColor(const QColor &color);
    int mode() const { return m_mode; }
    void setMode(int mode);
    qreal lineWidth() const { return m_lineWidth; }
    void setLineWidth(qreal width);
    int trailDepth() const { return m_trailDepth; }
    void setTrailDepth(int depth);
    qreal fillOpacity() const { return m_fillOpacity; }
    void setFillOpacity(qreal opacity);

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) override;

private:
    QVariantList m_points;
    QVector<QVariantList> m_history;
    QColor m_traceColor{ 63, 217, 200 };
    int m_mode = 0;
    qreal m_lineWidth = 1.5;
    int m_trailDepth = 4;
    qreal m_fillOpacity = 0.14;
};

extern "C" void chilpa_scope_trace_register_qml_type();

#endif
