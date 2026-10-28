#pragma once
#include <QtCore/QVariantList>
#include <QtGui/QColor>
#include <QtQuick/QQuickItem>
#include <vector>

// The waveform's bars as ONE scene-graph node: a quad per column from its
// downward peak to its upward one, colored by whether the column is played,
// analyzed or not yet. A Rectangle per column made two thousand items whose
// bindings all re-ran on every position tick and every refresh of a growing
// document; here a tick only rewrites the vertex colors.
class WaveformBars : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QVariantList values READ values WRITE setValues)
    Q_PROPERTY(QVariantList lows READ lows WRITE setLows)
    Q_PROPERTY(qreal played READ played WRITE setPlayed)
    Q_PROPERTY(qreal analyzed READ analyzed WRITE setAnalyzed)
    Q_PROPERTY(qreal slot READ slot WRITE setSlot)
    Q_PROPERTY(qreal barWide READ barWide WRITE setBarWide)
    Q_PROPERTY(qreal halfTall READ halfTall WRITE setHalfTall)
    Q_PROPERTY(qreal barLeast READ barLeast WRITE setBarLeast)
    Q_PROPERTY(QColor playedColor READ playedColor WRITE setPlayedColor)
    Q_PROPERTY(QColor knownColor READ knownColor WRITE setKnownColor)
    Q_PROPERTY(QColor restColor READ restColor WRITE setRestColor)
    Q_PROPERTY(int count READ count NOTIFY countChanged)
public:
    explicit WaveformBars(QQuickItem *parent = nullptr);
    QVariantList values() const { return m_values; }
    QVariantList lows() const { return m_lows; }
    qreal played() const { return m_played; }
    qreal analyzed() const { return m_analyzed; }
    qreal slot() const { return m_slot; }
    qreal barWide() const { return m_barWide; }
    qreal halfTall() const { return m_halfTall; }
    qreal barLeast() const { return m_barLeast; }
    QColor playedColor() const { return m_playedColor; }
    QColor knownColor() const { return m_knownColor; }
    QColor restColor() const { return m_restColor; }
    int count() const { return int(m_up.size()); }
    void setValues(const QVariantList &values);
    void setLows(const QVariantList &lows);
    void setPlayed(qreal value);
    void setAnalyzed(qreal value);
    void setSlot(qreal value);
    void setBarWide(qreal value);
    void setHalfTall(qreal value);
    void setBarLeast(qreal value);
    void setPlayedColor(const QColor &value);
    void setKnownColor(const QColor &value);
    void setRestColor(const QColor &value);
    // Where a column is drawn and in what color, for the tests.
    Q_INVOKABLE QRectF barAt(int index) const;
    Q_INVOKABLE QColor colorAt(int index) const;
signals:
    void countChanged();
protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &next, const QRectF &previous) override;
private:
    static std::vector<float> clamped(const QVariantList &list);
    bool matched() const { return !m_up.empty() && m_up.size() == m_down.size(); }
    QColor color(int index) const;
    QVariantList m_values, m_lows;
    std::vector<float> m_up, m_down;
    qreal m_played = 0, m_analyzed = 0, m_slot = 1, m_barWide = 1, m_halfTall = 1, m_barLeast = 1;
    QColor m_playedColor, m_knownColor, m_restColor;
    bool m_shape = true;
    bool m_colors = true;
};

extern "C" void chilpa_waveform_bars_register_qml_type();
