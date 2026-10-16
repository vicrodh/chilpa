// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
// Plasma (immersive shader scene, mode 1) — the QQuickRhiItem behind the
// feedback fluid. Block A2 of the 2026-08-15 immersive-completion contract
// (spec 01 §2.1). Second hand-written C++ item after LineBedItem
// (cxx/linebed_item.h carries the why-C++ rationale: a ShaderEffect cannot
// own a feedback texture pair).
//
// QML properties are MEMBER-only (no NOTIFY): the QML wrapper
// (PlasmaScene.qml) writes them on the pulse edge and calls update()
// itself — the same cadence contract as LineBedItem.
//
// Registered as QML type `PlasmaItem` in `app.chilpa.graphics` 1.0 (see the
// .cpp).

#pragma once

#include <QtGui/QColor>
#include <QtGui/QVector4D>
#include <QtQuick/QQuickRhiItem>

class PlasmaItem : public QQuickRhiItem
{
    Q_OBJECT
    Q_PROPERTY(float time MEMBER m_time)
    Q_PROPERTY(float beat MEMBER m_beat)
    Q_PROPERTY(float level MEMBER m_level)
    Q_PROPERTY(float levelSmooth MEMBER m_levelSmooth)
    Q_PROPERTY(QVector4D energyLo MEMBER m_energyLo)
    Q_PROPERTY(QVector4D energyHi MEMBER m_energyHi)
    // THE FAST HALF OF THE PACK, which used to arrive zeroed.
    //
    // `plasma.frag` has always declared `transientAmp`, `bandsLo` and
    // `bandsHi` in its std140 block and the pack has always published them —
    // `ShaderSceneLayer` even parses and stores all three. They simply never
    // reached this item, so the shader read zeros and ran entirely off the
    // ENERGY bands, which decay x0.85 per frame (~473 ms to a tenth) against
    // the bars' x0.65 (~178 ms). Plasma was being driven by the slowest
    // signals in the pipeline while the fast ones travelled beside them
    // unconnected.
    Q_PROPERTY(float transientAmp MEMBER m_transientAmp)
    Q_PROPERTY(QVector4D bandsLo MEMBER m_bandsLo)
    Q_PROPERTY(QVector4D bandsHi MEMBER m_bandsHi)
    Q_PROPERTY(QColor primary MEMBER m_primary)
    Q_PROPERTY(QColor secondary MEMBER m_secondary)
    Q_PROPERTY(QColor accent MEMBER m_accent)

public:
    explicit PlasmaItem(QQuickItem *parent = nullptr) : QQuickRhiItem(parent) {}

    QQuickRhiItemRenderer *createRenderer() override;

    // Member snapshots the renderer reads in synchronize(). Defaults are the
    // Slint palette (shader_underlay.rs:121-129) so a pre-art track still
    // looks right.
    float m_time = 0.0f;
    float m_beat = 0.0f;
    float m_level = 0.0f;
    float m_levelSmooth = 0.0f;
    QVector4D m_energyLo{ 0.0f, 0.0f, 0.0f, 0.0f };
    QVector4D m_energyHi{ 0.0f, 0.0f, 0.0f, 0.0f };
    float m_transientAmp = 0.0f;
    QVector4D m_bandsLo{ 0.0f, 0.0f, 0.0f, 0.0f };
    QVector4D m_bandsHi{ 0.0f, 0.0f, 0.0f, 0.0f };
    QColor m_primary{ 0, 220, 200 };
    QColor m_secondary{ 150, 50, 255 };
    QColor m_accent{ 63, 217, 200 };
};
