#include "waveform_bars.h"
#include <QtCore/QCoreApplication>
#include <QtQml/qqml.h>
#include <QtQuick/QSGGeometryNode>
#include <QtQuick/QSGVertexColorMaterial>
#include <algorithm>
#include <cmath>

namespace {
constexpr int verticesPerBar = 6;
// Below this a column has no bar at all.
constexpr float silence = 0.0005f;

// The scene graph's vertex colors are premultiplied.
void paint(QSGGeometry::ColoredPoint2D &vertex, const QColor &color) {
    const qreal alpha = color.alphaF();
    vertex.r = uchar(std::lround(color.redF() * alpha * 255));
    vertex.g = uchar(std::lround(color.greenF() * alpha * 255));
    vertex.b = uchar(std::lround(color.blueF() * alpha * 255));
    vertex.a = uchar(std::lround(alpha * 255));
}
}

WaveformBars::WaveformBars(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
}

std::vector<float> WaveformBars::clamped(const QVariantList &list) {
    std::vector<float> out;
    out.reserve(size_t(list.size()));
    for (const QVariant &item : list)
        out.push_back(std::clamp(item.toFloat(), 0.0f, 1.0f));
    return out;
}

void WaveformBars::setValues(const QVariantList &values) {
    if (values == m_values) return;
    const int before = count();
    m_values = values;
    m_up = clamped(values);
    m_shape = true;
    update();
    if (before != count()) emit countChanged();
}

void WaveformBars::setLows(const QVariantList &lows) {
    if (lows == m_lows) return;
    m_lows = lows;
    m_down = clamped(lows);
    m_shape = true;
    update();
}

#define CHILPA_SETTER(name, member, flag) \
    void WaveformBars::name(qreal value) { \
        if (qFuzzyCompare(member + 1, value + 1)) return; \
        member = value; flag = true; update(); \
    }
CHILPA_SETTER(setPlayed, m_played, m_colors)
CHILPA_SETTER(setAnalyzed, m_analyzed, m_colors)
CHILPA_SETTER(setSlot, m_slot, m_shape)
CHILPA_SETTER(setBarWide, m_barWide, m_shape)
CHILPA_SETTER(setHalfTall, m_halfTall, m_shape)
CHILPA_SETTER(setBarLeast, m_barLeast, m_shape)
#undef CHILPA_SETTER

#define CHILPA_COLOR(name, member) \
    void WaveformBars::name(const QColor &value) { \
        if (member == value) return; \
        member = value; m_colors = true; update(); \
    }
CHILPA_COLOR(setPlayedColor, m_playedColor)
CHILPA_COLOR(setKnownColor, m_knownColor)
CHILPA_COLOR(setRestColor, m_restColor)
#undef CHILPA_COLOR

void WaveformBars::geometryChange(const QRectF &next, const QRectF &previous) {
    QQuickItem::geometryChange(next, previous);
    if (next.height() != previous.height()) { m_shape = true; update(); }
}

QColor WaveformBars::color(int index) const {
    const qreal columns = qreal(m_up.size());
    if ((index + 1) / columns <= m_played) return m_playedColor;
    if (index / columns < m_analyzed) return m_knownColor;
    return m_restColor;
}

QColor WaveformBars::colorAt(int index) const {
    return index >= 0 && index < count() ? color(index) : QColor();
}

QRectF WaveformBars::barAt(int index) const {
    if (!matched() || index < 0 || index >= count()) return {};
    const qreal least = m_barLeast / 2;
    const qreal room = std::max<qreal>(0, m_halfTall - least);
    const qreal reach = std::min(m_halfTall, least + m_up[size_t(index)] * room);
    const qreal drop = std::min(m_halfTall, least + m_down[size_t(index)] * room);
    return QRectF(m_slot * index, height() / 2 - reach, m_barWide, reach + drop);
}

QSGNode *WaveformBars::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) {
    auto *node = static_cast<QSGGeometryNode *>(oldNode);
    if (!matched() || width() <= 0 || height() <= 0) {
        delete node;
        m_shape = m_colors = true;
        return nullptr;
    }
    const int bars = count();
    const int total = bars * verticesPerBar;
    if (!node) {
        node = new QSGGeometryNode;
        auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), total);
        geometry->setDrawingMode(QSGGeometry::DrawTriangles);
        node->setGeometry(geometry);
        node->setFlag(QSGNode::OwnsGeometry);
        node->setMaterial(new QSGVertexColorMaterial);
        node->setFlag(QSGNode::OwnsMaterial);
        m_shape = m_colors = true;
    }
    QSGGeometry *geometry = node->geometry();
    if (geometry->vertexCount() != total) {
        geometry->allocate(total);
        m_shape = m_colors = true;
    }
    auto *vertex = geometry->vertexDataAsColoredPoint2D();
    for (int i = 0; i < bars; ++i, vertex += verticesPerBar) {
        if (m_shape) {
            const bool drawn = m_up[size_t(i)] > silence || m_down[size_t(i)] > silence;
            const QRectF r = drawn ? barAt(i) : QRectF(m_slot * i, height() / 2, m_barWide, 0);
            const float x0 = float(r.left()), x1 = float(r.right());
            const float y0 = float(r.top()), y1 = float(r.bottom());
            vertex[0].set(x0, y0, 0, 0, 0, 0);
            vertex[1].set(x1, y0, 0, 0, 0, 0);
            vertex[2].set(x0, y1, 0, 0, 0, 0);
            vertex[3].set(x1, y0, 0, 0, 0, 0);
            vertex[4].set(x1, y1, 0, 0, 0, 0);
            vertex[5].set(x0, y1, 0, 0, 0, 0);
        }
        if (m_shape || m_colors) {
            const QColor c = color(i);
            for (int v = 0; v < verticesPerBar; ++v) paint(vertex[v], c);
        }
    }
    m_shape = m_colors = false;
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

extern "C" void chilpa_waveform_bars_register_qml_type() {
    static bool registered = false;
    if (registered) return;
    registered = true;
    qmlRegisterType<WaveformBars>("app.chilpa.waveform", 1, 0, "WaveformBars");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_waveform_bars_register_qml_type)
