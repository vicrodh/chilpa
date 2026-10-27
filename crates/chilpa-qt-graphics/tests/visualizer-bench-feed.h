#ifndef VISUALIZER_BENCH_FEED_H
#define VISUALIZER_BENCH_FEED_H
#include <QtCore/QObject>
#include <QtCore/QList>

// I expose the same bounded typed sequence boundary to both renderers.
class VisualizerBenchFeed final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QList<float> bars READ bars NOTIFY barsChanged)
    Q_PROPERTY(QList<float> waveform READ waveform NOTIFY waveformChanged)
    Q_PROPERTY(QList<float> energy READ energy NOTIFY energyChanged)
    Q_PROPERTY(QList<float> goniometer READ goniometer NOTIFY goniometerChanged)
    Q_PROPERTY(QList<float> oscilloscope READ oscilloscope NOTIFY oscilloscopeChanged)
    Q_PROPERTY(float stereoCorrelation READ stereoCorrelation NOTIFY stereoCorrelationChanged)
public:
    explicit VisualizerBenchFeed(QObject *parent = nullptr) : QObject(parent) {}
    QList<float> bars() const { return m_bars; }
    QList<float> waveform() const { return m_waveform; }
    QList<float> energy() const { return m_energy; }
    QList<float> goniometer() const { return m_goniometer; }
    QList<float> oscilloscope() const { return m_oscilloscope; }
    float stereoCorrelation() const { return m_correlation; }
    Q_INVOKABLE void configure(bool reference, int mode, uint rate, qulonglong held, bool compact);
    Q_INVOKABLE void setEnabled(bool enabled);
    Q_INVOKABLE void setCompact(bool presented, int mode);
    Q_INVOKABLE void poll();
    Q_INVOKABLE qulonglong submitted() const;
    Q_INVOKABLE qulonglong overwritten() const;
signals:
    void barsChanged();
    void waveformChanged();
    void energyChanged();
    void goniometerChanged();
    void oscilloscopeChanged();
    void stereoCorrelationChanged();
private:
    int m_mode = 0;
    float m_correlation = 0;
    QList<float> m_bars, m_waveform, m_energy, m_goniometer, m_oscilloscope;
};
#endif
