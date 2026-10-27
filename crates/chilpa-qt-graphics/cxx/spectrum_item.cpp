#include "spectrum_item.h"
#include <algorithm>
#include <cmath>
#include <QtCore/QCoreApplication>
#include <QtQml/qqml.h>
#include <QtQuick/QSGGeometryNode>
#include <QtQuick/QSGVertexColorMaterial>

namespace {
constexpr int kColumns = 28;
constexpr int kMaxRows = 128;
using Frame = std::array<float, 16>;
class SpectrumNode final : public QSGGeometryNode {
public:
    int capacity = 0;
};
Frame bounded(const QVariantList &source)
{
    Frame result{};
    for (int i = 0; i < std::min<int>(source.size(), result.size()); ++i) {
        const float value = source[i].toFloat();
        result[i] = std::isfinite(value) ? std::clamp(value, 0.0f, 1.0f) : 0.0f;
    }
    return result;
}
QVariantList list(const Frame &source)
{
    QVariantList result;
    result.reserve(source.size());
    for (float value : source) result.append(value);
    return result;
}
using Color = std::array<float, 4>;
Color premultiplied(const QColor &color)
{
    const float alpha = color.alphaF();
    return {float(color.redF()) * alpha, float(color.greenF()) * alpha, float(color.blueF()) * alpha, alpha};
}
Color gradient(const Color &top, const Color &bottom, float position)
{
    Color result;
    for (int i = 0; i < 4; ++i) result[i] = top[i] + (bottom[i] - top[i]) * std::clamp(position, 0.0f, 1.0f);
    return result;
}
void vertex(QSGGeometry::ColoredPoint2D &out, float x, float y, const Color &color)
{
    out.set(x, y, std::lround(color[0] * 255), std::lround(color[1] * 255),
            std::lround(color[2] * 255), std::lround(color[3] * 255));
}
void rectangle(QSGGeometry::ColoredPoint2D *&out, float left, float right, float top, float bottom,
               const Color &upper, const Color &lower)
{
    if (bottom <= top) return;
    vertex(out[0], left, top, upper); out[1] = out[0];
    out[2] = out[0]; out[2].x = right;
    vertex(out[3], left, bottom, lower); out[4] = out[3]; out[4].x = right; out[5] = out[4];
    out += 6;
}
}

SpectrumItem::SpectrumItem(QQuickItem *parent) : QQuickItem(parent) { setFlag(ItemHasContents, true); }
QVariantList SpectrumItem::values() const { return list(m_values); }
QVariantList SpectrumItem::peakValues() const { return list(m_peaks); }
void SpectrumItem::setValues(const QVariantList &values) { const auto next = bounded(values); if (next != m_values) { m_values = next; update(); } }
void SpectrumItem::setPeakValues(const QVariantList &values) { const auto next = bounded(values); if (next != m_peaks) { m_peaks = next; update(); } }
void SpectrumItem::setTopColor(const QColor &value) { if (m_top != value) { m_top = value; update(); } }
void SpectrumItem::setBottomColor(const QColor &value) { if (m_bottom != value) { m_bottom = value; update(); } }
void SpectrumItem::setMode(int value) { if (m_mode != value) { m_mode = value; update(); } }
void SpectrumItem::geometryChange(const QRectF &next, const QRectF &previous)
{
    QQuickItem::geometryChange(next, previous);
    if (next.size() != previous.size()) update();
}

QSGNode *SpectrumItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *)
{
    const float w = width(), h = height(), slot = w / kColumns;
    // The host uses the shader for exceptionally narrow/tall surfaces. This
    // second guard bounds native storage even for malformed standalone inputs.
    if (!std::isfinite(w) || !std::isfinite(h) || slot <= 2 || h <= 0 || h / slot + 0.5f > kMaxRows) {
        delete oldNode;
        return nullptr;
    }
    const int rows = int(std::floor(h / slot + 0.5f));
    // One gradient clamp split, one peak split and one mark per column. Capacity stays
    // fixed between resizes: changing amplitudes never allocates geometry.
    const int capacity = kColumns * (rows + 3) * 6;
    auto *node = static_cast<SpectrumNode *>(oldNode);
    if (!node) {
        node = new SpectrumNode;
        node->capacity = capacity;
        auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), capacity);
        geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
        geometry->setVertexDataPattern(QSGGeometry::DynamicPattern);
        node->setGeometry(geometry);
        node->setFlag(QSGNode::OwnsGeometry);
        node->setMaterial(new QSGVertexColorMaterial);
        node->setFlag(QSGNode::OwnsMaterial);
    }
    auto *material = node->material();
    const bool blending = m_top.alphaF() < 1.0 || m_bottom.alphaF() < 1.0;
    if (material->flags().testFlag(QSGMaterial::Blending) != blending) {
        material->setFlag(QSGMaterial::Blending, blending);
        node->markDirty(QSGNode::DirtyMaterial);
    }
    auto *geometry = node->geometry();
    if (node->capacity != capacity) { geometry->allocate(capacity); node->capacity = capacity; }
    auto *begin = geometry->vertexDataAsColoredPoint2D();
    auto *out = begin;
    const Color top = premultiplied(m_top), bottom = premultiplied(m_bottom);
    for (int column = 0; column < kColumns; ++column) {
        int bin = column < 14 ? column : 27 - column;
        if (bin > 0) ++bin;
        const float amplitude = m_values[bin] * h;
        const float left = column * slot + 1, right = (column + 1) * slot - 1;
        const bool peak = m_mode == 5 && m_peaks[bin] >= 0.001f;
        const float peakTop = std::max(0.0f, h * (1 - m_peaks[bin]) - 1);
        const auto tint = [&](float y) { return gradient(top, bottom, (y - (h - amplitude)) / std::max(amplitude, 1.0f)); };
        const auto segment = [&](float upper, float lower) {
            if (lower <= upper) return;
            const float clampY = std::clamp(h - amplitude, upper, lower);
            if (clampY > upper && clampY < lower) {
                rectangle(out, left, right, upper, clampY, top, top);
                rectangle(out, left, right, clampY, lower, top, tint(lower));
            } else {
                rectangle(out, left, right, upper, lower, tint(upper), tint(lower));
            }
        };
        for (int row = 0; row < rows; ++row) {
            if ((row + 0.5f) * slot > amplitude) break;
            const float cellTop = std::max(0.0f, h - (row + 1) * slot + 1);
            const float cellBottom = h - row * slot - 1;
            if (peak && peakTop < cellBottom && peakTop + 1 > cellTop) {
                // Avoid drawing translucent cells twice under the peak mark.
                const float upperEnd = std::max(cellTop, peakTop);
                const float lowerStart = std::min(cellBottom, peakTop + 1);
                segment(cellTop, upperEnd);
                segment(lowerStart, cellBottom);
            } else {
                segment(cellTop, cellBottom);
            }
        }
        if (peak) rectangle(out, left, right, peakTop, peakTop + 1, top, top);
    }
#if QT_VERSION >= QT_VERSION_CHECK(6, 10, 0)
    // A degenerate triangle keeps Qt able to revive the node after silence.
    const int drawn = static_cast<int>(out - begin);
    if (drawn == 0) std::fill(begin, begin + 3, QSGGeometry::ColoredPoint2D{});
    geometry->setVertexCount(std::max(3, drawn));
#else
    std::fill(out, begin + capacity, QSGGeometry::ColoredPoint2D{});
#endif
    geometry->markVertexDataDirty();
    node->markDirty(QSGNode::DirtyGeometry);
    return node;
}

extern "C" void chilpa_spectrum_register_qml_type()
{
    static bool registered = false;
    if (registered) return;
    registered = true;
    qmlRegisterType<SpectrumItem>("app.chilpa.graphics", 1, 0, "SpectrumItem");
}
Q_COREAPP_STARTUP_FUNCTION(chilpa_spectrum_register_qml_type)
