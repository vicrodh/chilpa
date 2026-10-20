#include "playing_surface.h"
#include <QtCore/QCoreApplication>
#include <QtGui/QPainter>
#include <QtGui/QRasterWindow>
#include <QtQml/qqml.h>
#include <QtQml/QQmlProperty>
#include <QtQuick/QQuickWindow>
#include <algorithm>
#include <cmath>

namespace {
// I bound native surfaces across windows, and release them when hidden or covered.
constexpr int maxSurfaces = 4;
constexpr qreal maxScale = 4;
constexpr qreal maxDpr = 4;
int surfaces = 0;

class RasterBars : public QRasterWindow {
public:
    qreal phase = 0, scale = 1;
    QColor tint;
    QPointF offset;
    RasterBars() {
        ++surfaces;
        auto format = requestedFormat();
        format.setAlphaBufferSize(8);
        setFormat(format);
        setFlags(Qt::FramelessWindowHint | Qt::WindowTransparentForInput | Qt::WindowDoesNotAcceptFocus);
    }
    ~RasterBars() override { --surfaces; }
protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.setCompositionMode(QPainter::CompositionMode_Source);
        painter.fillRect(QRect(0, 0, width(), height()), Qt::transparent);
        painter.setCompositionMode(QPainter::CompositionMode_SourceOver);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setPen(Qt::NoPen);
        painter.setBrush(tint);
        painter.translate(offset);
        constexpr qreal amplitudes[] = {0.6, 1, 0.4, 0.8};
        constexpr qreal phases[] = {0, 0.25, 0.125, 0.375};
        for (int i = 0; i < 4; ++i) {
            const qreal height = 14 * scale * amplitudes[i]
                * (0.3 + 0.7 * (0.5 - 0.5 * std::cos((phase + phases[i]) * 6.283185)));
            const qreal radius = std::min(scale, height * 0.5);
            painter.drawRoundedRect(QRectF((1 + 5 * i) * scale, 15 * scale - height, 3 * scale, height), radius, radius);
        }
    }
};

// I conservatively keep intersecting painted items in the scene graph's stacking order.
bool covers(QQuickItem *item, const QRectF &area) {
    if (!item->isVisible() || item->opacity() <= 0) return false;
    const QRectF local = item->mapRectFromScene(area);
    if (item->clip() && !item->clipRect().intersects(local)) return false;
    if (item->flags().testFlag(QQuickItem::ItemHasContents) && item->boundingRect().intersects(local)) return true;
    for (QQuickItem *child : item->childItems())
        if (covers(child, area)) return true;
    return false;
}
}

PlayingSurface::PlayingSurface(QQuickItem *parent) : QQuickItem(parent) {
    connect(this, &QQuickItem::windowChanged, this, &PlayingSurface::watchWindow);
}

PlayingSurface::~PlayingSurface() {
    if (m_surface) { m_surface->hide(); m_surface->deleteLater(); }
}

void PlayingSurface::setActive(bool value) {
    if (m_active == value) return;
    m_active = value;
    emit activeChanged();
    watchWindow();
    if (!m_active) releaseSurface();
}

void PlayingSurface::setPhase(qreal value) {
    if (m_phase == value) return;
    m_phase = value;
    emit phaseChanged();
    if (auto *raster = static_cast<RasterBars *>(m_surface.data())) { raster->phase = value; raster->update(); }
}

void PlayingSurface::setBarScale(qreal value) {
    if (m_scale == value) return;
    m_scale = value;
    emit barScaleChanged();
    polish();
    if (auto *raster = static_cast<RasterBars *>(m_surface.data())) { raster->scale = value; raster->update(); }
}

void PlayingSurface::setTint(const QColor &value) {
    if (m_tint == value) return;
    m_tint = value;
    emit tintChanged();
    if (auto *raster = static_cast<RasterBars *>(m_surface.data())) { raster->tint = value; raster->update(); }
}

void PlayingSurface::watchWindow() {
    disconnect(m_frame);
    if (m_active && window()) {
        // I check only on frames the containing scene already needs.
        m_frame = connect(window(), &QQuickWindow::afterAnimating, this, &PlayingSurface::updatePolish);
        polish();
    } else {
        releaseSurface();
    }
}

bool PlayingSurface::exposed() const {
    if (!m_active || !isVisible() || !window() || !window()->isExposed()
            || m_scale <= 0 || m_scale > maxScale || window()->devicePixelRatio() > maxDpr
            || width() <= 0 || height() <= 0 || width() > 80 || height() > 64) return false;
    bool valid = false;
    if (itemTransform(window()->contentItem(), &valid).type() > QTransform::TxTranslate || !valid) return false;
    const QRectF area = mapRectToScene(boundingRect());
    if (!QRectF(0, 0, window()->width(), window()->height()).contains(area)) return false;
    // My parent owns both renderers; neither is an occluder of its replacement.
    for (QQuickItem *item = parentItem(); item; item = item->parentItem()) {
        if (!item->isVisible() || item->opacity() != 1) return false;
        if (QQmlProperty(item, QStringLiteral("layer.enabled")).read().toBool()) return false;
        if (item->clip() && !item->clipRect().contains(item->mapRectFromScene(area))) return false;
        QQuickItem *parent = item->parentItem();
        if (!parent) break;
        if (item->z() < 0 && parent->flags().testFlag(ItemHasContents)
                && parent->boundingRect().intersects(parent->mapRectFromScene(area))) return false;
        bool after = false;
        for (QQuickItem *sibling : parent->childItems()) {
            if (sibling == item) { after = true; continue; }
            if ((sibling->z() > item->z() || (after && sibling->z() == item->z())) && covers(sibling, area)) return false;
        }
    }
    return true;
}

void PlayingSurface::releaseSurface() {
    if (!m_surface) return;
    auto *old = m_surface.data();
    old->hide();
    m_surface = nullptr;
    emit surfaceChanged();
    // I release after the current scene update finishes.
    old->deleteLater();
}

void PlayingSurface::updatePolish() {
    if (!exposed()) { releaseSurface(); return; }
    auto *raster = static_cast<RasterBars *>(m_surface.data());
    if (!raster) {
        if (surfaces >= maxSurfaces) return;
        raster = new RasterBars;
        raster->phase = m_phase;
        raster->scale = m_scale;
        raster->tint = m_tint;
        raster->setParent(window());
        m_surface = raster;
        emit surfaceChanged();
    }
    const QRectF area = mapRectToScene(boundingRect());
    const QRect geometry = area.toAlignedRect();
    raster->offset = area.topLeft() - geometry.topLeft();
    raster->setGeometry(geometry);
    raster->show();
}

extern "C" void chilpa_playing_surface_register_qml_type() {
    static bool registered = false;
    if (registered) return;
    registered = true;
    qmlRegisterType<PlayingSurface>("app.chilpa.kit", 1, 0, "PlayingSurface");
}
Q_COREAPP_STARTUP_FUNCTION(chilpa_playing_surface_register_qml_type)
