// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
// Tunnel Flow (immersive shader scene, mode 8 — Qt-only) — the
// QQuickRhiItem behind the feedback tunnel. Block B1 of the 2026-08-15
// immersive-completion contract (spec 02-tauri-tunnel-port.md): a rewrite of
// the legacy Tauri Canvas2D panel (TunnelFlowPanel.svelte) as
// qml/assets/shaders/tunnel_flow.frag. Same mechanism as PlasmaItem
// (cxx/plasma_item.h): a feedback ping-pong over two owned RGBA8 textures a
// ShaderEffect cannot own.
//
// QML properties are MEMBER-only (no NOTIFY): the QML wrapper
// (TunnelFlowScene.qml) computes the audio state (Viz16 smoothing, kick
// detector, the TAURI phase accumulator) and writes these on the pulse edge
// via pulseTick() — the same cadence contract as PlasmaItem/LineBedItem.
//
// Registered as QML type `TunnelFlowItem` in `app.chilpa.graphics` 1.0 (see the
// .cpp).

#pragma once

#include <QtGui/QColor>
#include <QtQuick/QQuickRhiItem>

class TunnelFlowItem : public QQuickRhiItem
{
    Q_OBJECT
    Q_PROPERTY(float time MEMBER m_time)
    Q_PROPERTY(float phase MEMBER m_phase)
    Q_PROPERTY(float bass MEMBER m_bass)
    Q_PROPERTY(float mid MEMBER m_mid)
    Q_PROPERTY(float high MEMBER m_high)
    Q_PROPERTY(float kick MEMBER m_kick)
    Q_PROPERTY(QColor palette0 MEMBER m_palette0)
    Q_PROPERTY(QColor palette1 MEMBER m_palette1)
    Q_PROPERTY(QColor palette2 MEMBER m_palette2)
    Q_PROPERTY(QColor palette3 MEMBER m_palette3)

public:
    explicit TunnelFlowItem(QQuickItem *parent = nullptr) : QQuickRhiItem(parent) {}

    QQuickRhiItemRenderer *createRenderer() override;

    // Member snapshots the renderer reads in synchronize(). The palette
    // defaults are the Tauri fallback (TunnelFlowPanel.svelte:63-68) so a
    // pre-art track still looks right; time is SECONDS here (the shader
    // converts to the source's ms domain).
    float m_time = 0.0f;
    float m_phase = 0.0f;
    float m_bass = 0.0f;
    float m_mid = 0.0f;
    float m_high = 0.0f;
    float m_kick = 0.0f;
    QColor m_palette0{ 255, 106, 106 };
    QColor m_palette1{ 255, 205, 92 };
    QColor m_palette2{ 104, 220, 170 };
    QColor m_palette3{ 110, 176, 255 };
};
