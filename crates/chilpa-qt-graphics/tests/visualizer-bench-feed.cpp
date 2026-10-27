#include "visualizer-bench-feed.h"
#include <QtCore/QCoreApplication>
#include <QtQml/qqml.h>
#include <array>
#include <cstddef>
#include <cstdint>

extern "C" {
void visualizer_bench_configure(bool reference, int mode, std::uint32_t rate, std::uint64_t held, bool compact);
void visualizer_bench_enabled(bool enabled);
std::size_t visualizer_bench_poll(float *out, std::size_t capacity, float *correlation);
std::uint64_t visualizer_bench_submitted();
std::uint64_t visualizer_bench_overwritten();
}

void VisualizerBenchFeed::configure(bool reference, int mode, uint rate, qulonglong held, bool compact)
{
    m_mode = mode;
    visualizer_bench_configure(reference, mode, rate, held, compact);
}
void VisualizerBenchFeed::setEnabled(bool enabled) { visualizer_bench_enabled(enabled); }
// The fixture owns a common visibility gate, as each application's host does.
void VisualizerBenchFeed::setCompact(bool, int) {}
qulonglong VisualizerBenchFeed::submitted() const { return visualizer_bench_submitted(); }
qulonglong VisualizerBenchFeed::overwritten() const { return visualizer_bench_overwritten(); }

void VisualizerBenchFeed::poll()
{
    std::array<float, 512> samples{};
    float correlation = 0;
    const auto count = visualizer_bench_poll(samples.data(), samples.size(), &correlation);
    if (!count) return;
    QList<float> frame;
    frame.reserve(static_cast<qsizetype>(count));
    for (std::size_t i = 0; i < count; ++i) frame.append(samples[i]);
    switch (m_mode) {
    case 1: m_waveform = std::move(frame); emit waveformChanged(); break;
    case 2: m_energy = std::move(frame); emit energyChanged(); break;
    case 3:
        m_goniometer = std::move(frame); emit goniometerChanged();
        m_correlation = correlation; emit stereoCorrelationChanged(); break;
    case 4: m_oscilloscope = std::move(frame); emit oscilloscopeChanged(); break;
    default: m_bars = std::move(frame); emit barsChanged(); break;
    }
}

static void registerFeed()
{
    qmlRegisterSingletonType<VisualizerBenchFeed>("com.blitzfc.qbz", 1, 0, "QbzViz",
        [](QQmlEngine *, QJSEngine *) -> QObject * { return new VisualizerBenchFeed; });
}
Q_COREAPP_STARTUP_FUNCTION(registerFeed)
