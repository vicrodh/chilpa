// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
// Line Bed (immersive shader scene, mode 5) — the GPU half. Block A4 of the
// 2026-08-15 immersive-completion contract (spec 01-shader-scenes-port.md
// §2.5). The project's FIRST QQuickRhiItem: the scene is not expressible as
// a ShaderEffect (fragment-only fullscreen quad) — it needs a LineStrip
// vertex stage with instancing and a vertex-sampled heights texture, the
// exact shape of the Slint linestrip pipeline
// (crates/qbz/src/shader_underlay.rs:689-727).
//
// The CPU half (the 512→256 reshape + the 200-deep ring) is Rust
// (src/linebed_qt.rs, a LineBedState port); the ring arrives per pulse tick
// through the `heights` property as raw f32 bytes (256x200, row 0 =
// newest). The palette rides the shell's ambient colors like every other
// scene (QbzShell.ambientPrimary/Accent, bound in LineBedScene.qml).
//
// Compiled by crates/qbz-qt/build.rs (moc via qt-build-utils + cc — the
// first hand-written C++ in this crate; cxx-qt's generated glue aside).
// Registered as QML type `LineBedItem` in `app.chilpa.graphics` 1.0 at
// QGuiApplication construction (Q_COREAPP_STARTUP_FUNCTION in the .cpp),
// with `chilpa_linebed_register_qml_type` as the Rust-reachable link anchor.

#ifndef CHILPA_LINEBED_ITEM_H
#define CHILPA_LINEBED_ITEM_H

#include <QtCore/QByteArray>
#include <QtGui/QColor>
#include <QtQuick/QQuickRhiItem>

class LineBedItem : public QQuickRhiItem
{
    Q_OBJECT
    // No NOTIFY anywhere: the QML wrapper writes `heights` only on the
    // pulse edge and calls update() right after (the pulse law — nothing
    // here may schedule its own repaint), and nobody binds these.
    Q_PROPERTY(QByteArray heights READ heights WRITE setHeights)
    Q_PROPERTY(QColor primary READ primary WRITE setPrimary)
    Q_PROPERTY(QColor accent READ accent WRITE setAccent)

public:
    explicit LineBedItem(QQuickItem *parent = nullptr)
        : QQuickRhiItem(parent)
    {
    }

    QByteArray heights() const { return m_heights; }
    void setHeights(const QByteArray &heights) { m_heights = heights; }
    QColor primary() const { return m_primary; }
    void setPrimary(const QColor &c) { m_primary = c; }
    QColor accent() const { return m_accent; }
    void setAccent(const QColor &c) { m_accent = c; }

    QQuickRhiItemRenderer *createRenderer() override;

private:
    QByteArray m_heights;
    // The Slint defaults (shader_underlay.rs:121-129 Palette::DEFAULT
    // #00dcc8 / #3fd9c8 — `secondary` is unused by this scene), so the bed
    // opened before the first album-art palette resolves still looks right.
    QColor m_primary{ 0, 220, 200 };
    QColor m_accent{ 63, 217, 200 };
};

#endif // CHILPA_LINEBED_ITEM_H
