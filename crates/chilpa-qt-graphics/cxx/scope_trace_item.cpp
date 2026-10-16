// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
#include "scope_trace_item.h"

#include <algorithm>
#include <cmath>
#include <vector>

#include <QtCore/QCoreApplication>
#include <QtCore/QPointF>
#include <QtQml/qqml.h>
#include <QtQuick/QSGGeometryNode>
#include <QtQuick/QSGVertexColorMaterial>

namespace {

constexpr int kMaxTrailDepth = 5;

QSGGeometryNode *makeColorNode()
{
    auto *node = new QSGGeometryNode;
    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), 0);
    geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    node->setGeometry(geometry);
    node->setFlag(QSGNode::OwnsGeometry);
    node->setMaterial(new QSGVertexColorMaterial);
    node->setFlag(QSGNode::OwnsMaterial);
    return node;
}

class ScopeRootNode final : public QSGNode
{
public:
    ScopeRootNode()
    {
        fill = makeColorNode();
        appendChildNode(fill);
        for (int age = kMaxTrailDepth - 1; age >= 0; --age) {
            glow[age] = makeColorNode();
            core[age] = makeColorNode();
            appendChildNode(glow[age]);
            appendChildNode(core[age]);
        }
    }

    QSGGeometryNode *fill = nullptr;
    QSGGeometryNode *glow[kMaxTrailDepth]{};
    QSGGeometryNode *core[kMaxTrailDepth]{};
};

void setVertex(QSGGeometry::ColoredPoint2D &vertex,
               const QPointF &point,
               const QColor &color,
               qreal alpha)
{
    const qreal combined = qBound<qreal>(0.0, color.alphaF() * alpha, 1.0);
    vertex.set(static_cast<float>(point.x()),
               static_cast<float>(point.y()),
               static_cast<unsigned char>(std::round(color.red() * combined)),
               static_cast<unsigned char>(std::round(color.green() * combined)),
               static_cast<unsigned char>(std::round(color.blue() * combined)),
               static_cast<unsigned char>(std::round(255.0 * combined)));
}

qreal robustGain(const QVariantList &source, int mode)
{
    const int count = mode == 0 ? source.size() / 2 : source.size();
    if (count < 2)
        return 1.0;

    std::vector<float> magnitudes;
    magnitudes.reserve(count);
    for (int index = 0; index < count; ++index) {
        const float magnitude = mode == 0
            ? std::hypot(source[index * 2].toFloat(), source[index * 2 + 1].toFloat())
            : std::abs(source[index].toFloat());
        if (std::isfinite(magnitude))
            magnitudes.push_back(magnitude);
    }
    if (magnitudes.empty())
        return 1.0;

    const size_t percentile = static_cast<size_t>((magnitudes.size() - 1) * 0.96);
    std::nth_element(magnitudes.begin(), magnitudes.begin() + percentile, magnitudes.end());
    const qreal reference = magnitudes[percentile];
    if (reference < 0.025)
        return 1.0;
    const qreal target = mode == 0 ? 0.88 : 0.84;
    return qBound<qreal>(0.82, target / reference, mode == 0 ? 3.6 : 4.5);
}

QVector<QPointF> mapPoints(const QVariantList &source,
                           int mode,
                           qreal width,
                           qreal height,
                           qreal lineWidth)
{
    const int count = mode == 0 ? source.size() / 2 : source.size();
    QVector<QPointF> points;
    points.reserve(count);
    if (count < 2)
        return points;

    const qreal pad = qMax<qreal>(5.0, lineWidth * 3.8);
    const qreal drawWidth = qMax<qreal>(1.0, width - pad * 2.0);
    const qreal drawHeight = qMax<qreal>(1.0, height - pad * 2.0);
    const qreal gain = robustGain(source, mode);
    for (int index = 0; index < count; ++index) {
        if (mode == 0) {
            const qreal side = qBound<qreal>(-1.0, source[index * 2].toReal() * gain, 1.0);
            const qreal mid = qBound<qreal>(-1.0, source[index * 2 + 1].toReal() * gain, 1.0);
            points.push_back(QPointF(pad + (0.5 + side * 0.47) * drawWidth,
                                     pad + (0.5 - mid * 0.47) * drawHeight));
        } else {
            // The DSP publishes two pitch-locked periods. Repeat that stable
            // window according to the item's aspect ratio so a wide monitor
            // gains temporal detail instead of stretching two enormous
            // lobes. A square panel remains the original two-period scope.
            const int repeats = qBound(1, qRound(width / qMax<qreal>(1.0, height)), 4);
            const qreal wrapped = std::fmod(index * repeats, count - 1.0);
            const int first = qBound(0, static_cast<int>(std::floor(wrapped)), count - 1);
            const int second = qMin(first + 1, count - 1);
            const qreal fraction = wrapped - first;
            const qreal sample = source[first].toReal() * (1.0 - fraction)
                + source[second].toReal() * fraction;
            const qreal value = qBound<qreal>(-1.0, sample * gain, 1.0);
            points.push_back(QPointF(pad + index * drawWidth / qMax(1, count - 1),
                                     pad + (0.5 - value * 0.47) * drawHeight));
        }
    }
    return points;
}

void clearGeometry(QSGGeometryNode *node)
{
    if (node->geometry()->vertexCount() == 0)
        return;
    node->geometry()->allocate(0);
    node->markDirty(QSGNode::DirtyGeometry);
}

void setRibbon(QSGGeometryNode *node,
               const QVector<QPointF> &points,
               qreal width,
               const QColor &color,
               qreal alpha,
               bool fadeAlongTrace)
{
    if (points.size() < 2 || alpha <= 0.001) {
        clearGeometry(node);
        return;
    }

    auto *geometry = node->geometry();
    geometry->allocate(points.size() * 2);
    geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    auto *vertices = geometry->vertexDataAsColoredPoint2D();
    const qreal halfWidth = qMax<qreal>(0.5, width * 0.5);
    for (int index = 0; index < points.size(); ++index) {
        const QPointF before = points[qMax(0, index - 1)];
        const QPointF after = points[qMin(points.size() - 1, index + 1)];
        const QPointF tangent = after - before;
        const qreal length = std::hypot(tangent.x(), tangent.y());
        const QPointF normal = length > 0.0001
            ? QPointF(-tangent.y() / length * halfWidth,
                      tangent.x() / length * halfWidth)
            : QPointF(0.0, halfWidth);
        const qreal progress = points.size() > 1
            ? static_cast<qreal>(index) / (points.size() - 1)
            : 1.0;
        const qreal pointAlpha = alpha * (fadeAlongTrace ? 0.22 + progress * 0.78 : 1.0);
        setVertex(vertices[index * 2], points[index] + normal, color, pointAlpha);
        setVertex(vertices[index * 2 + 1], points[index] - normal, color, pointAlpha);
    }
    node->markDirty(QSGNode::DirtyGeometry);
}

void setPointCloud(QSGGeometryNode *node,
                   const QVector<QPointF> &points,
                   qreal diameter,
                   const QColor &color,
                   qreal alpha)
{
    if (points.isEmpty() || alpha <= 0.001) {
        clearGeometry(node);
        return;
    }

    auto *geometry = node->geometry();
    geometry->allocate(points.size() * 6);
    geometry->setDrawingMode(QSGGeometry::DrawTriangles);
    auto *vertices = geometry->vertexDataAsColoredPoint2D();
    const qreal radius = qMax<qreal>(0.5, diameter * 0.5);
    for (int index = 0; index < points.size(); ++index) {
        const QPointF point = points[index];
        const qreal progress = points.size() > 1
            ? static_cast<qreal>(index) / (points.size() - 1)
            : 1.0;
        const qreal pointAlpha = alpha * (0.18 + progress * 0.82);
        const QPointF topLeft(point.x() - radius, point.y() - radius);
        const QPointF topRight(point.x() + radius, point.y() - radius);
        const QPointF bottomLeft(point.x() - radius, point.y() + radius);
        const QPointF bottomRight(point.x() + radius, point.y() + radius);
        auto *quad = vertices + index * 6;
        setVertex(quad[0], topLeft, color, pointAlpha);
        setVertex(quad[1], bottomLeft, color, pointAlpha);
        setVertex(quad[2], topRight, color, pointAlpha);
        setVertex(quad[3], topRight, color, pointAlpha);
        setVertex(quad[4], bottomLeft, color, pointAlpha);
        setVertex(quad[5], bottomRight, color, pointAlpha);
    }
    node->markDirty(QSGNode::DirtyGeometry);
}

void setOscilloscopeFill(QSGGeometryNode *node,
                         const QVector<QPointF> &points,
                         qreal centreY,
                         const QColor &color,
                         qreal opacity)
{
    if (points.size() < 2 || opacity <= 0.001) {
        clearGeometry(node);
        return;
    }
    auto *geometry = node->geometry();
    geometry->allocate(points.size() * 2);
    geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    auto *vertices = geometry->vertexDataAsColoredPoint2D();
    for (int index = 0; index < points.size(); ++index) {
        setVertex(vertices[index * 2], points[index], color, opacity);
        setVertex(vertices[index * 2 + 1], QPointF(points[index].x(), centreY), color, 0.0);
    }
    node->markDirty(QSGNode::DirtyGeometry);
}

} // namespace

ScopeTraceItem::ScopeTraceItem(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
    setAntialiasing(true);
}

void ScopeTraceItem::setPoints(const QVariantList &points)
{
    if (m_points == points)
        return;
    m_points = points;
    m_history.prepend(points);
    while (m_history.size() > kMaxTrailDepth)
        m_history.removeLast();
    update();
}

void ScopeTraceItem::setTraceColor(const QColor &color)
{
    if (m_traceColor == color)
        return;
    m_traceColor = color;
    update();
}

void ScopeTraceItem::setMode(int mode)
{
    mode = mode == 1 ? 1 : 0;
    if (m_mode == mode)
        return;
    m_mode = mode;
    m_history.clear();
    if (!m_points.isEmpty())
        m_history.push_back(m_points);
    update();
}

void ScopeTraceItem::setLineWidth(qreal width)
{
    width = qMax<qreal>(1.0, width);
    if (qFuzzyCompare(m_lineWidth, width))
        return;
    m_lineWidth = width;
    update();
}

void ScopeTraceItem::setTrailDepth(int depth)
{
    depth = qBound(1, depth, kMaxTrailDepth);
    if (m_trailDepth == depth)
        return;
    m_trailDepth = depth;
    update();
}

void ScopeTraceItem::setFillOpacity(qreal opacity)
{
    opacity = qBound<qreal>(0.0, opacity, 0.4);
    if (qFuzzyCompare(m_fillOpacity, opacity))
        return;
    m_fillOpacity = opacity;
    update();
}

QSGNode *ScopeTraceItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    const int count = m_mode == 0 ? m_points.size() / 2 : m_points.size();
    if (count < 2 || width() <= 0.0 || height() <= 0.0) {
        delete oldNode;
        return nullptr;
    }

    auto *root = static_cast<ScopeRootNode *>(oldNode);
    if (!root)
        root = new ScopeRootNode;

    const QVector<QPointF> current = mapPoints(m_history.isEmpty() ? m_points : m_history[0],
                                               m_mode, width(), height(), m_lineWidth);
    setOscilloscopeFill(root->fill, current, height() * 0.5, m_traceColor,
                        m_mode == 1 ? m_fillOpacity : 0.0);

    static constexpr qreal trailAlpha[kMaxTrailDepth] = { 1.0, 0.42, 0.20, 0.09, 0.04 };
    for (int age = 0; age < kMaxTrailDepth; ++age) {
        if (age >= m_trailDepth || age >= m_history.size()) {
            clearGeometry(root->glow[age]);
            clearGeometry(root->core[age]);
            continue;
        }
        const QVector<QPointF> points = age == 0
            ? current
            : mapPoints(m_history[age], m_mode, width(), height(), m_lineWidth);
        const qreal alpha = trailAlpha[age];
        if (m_mode == 0) {
            setPointCloud(root->glow[age], points, m_lineWidth * 4.8,
                          m_traceColor, alpha * 0.10);
            setPointCloud(root->core[age], points, m_lineWidth * 1.25,
                          m_traceColor, alpha * 0.92);
        } else {
            setRibbon(root->glow[age], points, m_lineWidth * 5.2, m_traceColor,
                      alpha * 0.13, false);
            setRibbon(root->core[age], points, m_lineWidth, m_traceColor,
                      alpha * 0.96, false);
        }
    }
    return root;
}

static bool g_scopeTraceRegistered = false;

extern "C" void chilpa_scope_trace_register_qml_type()
{
    if (g_scopeTraceRegistered)
        return;
    g_scopeTraceRegistered = true;
    qmlRegisterType<ScopeTraceItem>("app.chilpa.graphics", 1, 0, "ScopeTraceItem");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_scope_trace_register_qml_type)
