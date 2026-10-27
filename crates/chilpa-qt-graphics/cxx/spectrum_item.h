#ifndef CHILPA_SPECTRUM_ITEM_H
#define CHILPA_SPECTRUM_ITEM_H

#include <array>
#include <QtCore/QVariantList>
#include <QtGui/QColor>
#include <QtQuick/QQuickItem>

// The existing spectrum state owns smoothing and peaks. This item only draws
// cells with Qt's vertex-color material, without a separate shader program.
class SpectrumItem : public QQuickItem
{
    Q_OBJECT
    Q_PROPERTY(QVariantList values READ values WRITE setValues)
    Q_PROPERTY(QVariantList peakValues READ peakValues WRITE setPeakValues)
    Q_PROPERTY(QColor topColor READ topColor WRITE setTopColor)
    Q_PROPERTY(QColor bottomColor READ bottomColor WRITE setBottomColor)
    Q_PROPERTY(int mode READ mode WRITE setMode)
public:
    explicit SpectrumItem(QQuickItem *parent = nullptr);
    QVariantList values() const;
    QVariantList peakValues() const;
    void setValues(const QVariantList &values);
    void setPeakValues(const QVariantList &values);
    QColor topColor() const { return m_top; }
    QColor bottomColor() const { return m_bottom; }
    void setTopColor(const QColor &value);
    void setBottomColor(const QColor &value);
    int mode() const { return m_mode; }
    void setMode(int value);
protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
private:
    std::array<float, 16> m_values{};
    std::array<float, 16> m_peaks{};
    QColor m_top{Qt::white};
    QColor m_bottom{Qt::white};
    int m_mode = 5;
};

extern "C" void chilpa_spectrum_register_qml_type();
#endif
