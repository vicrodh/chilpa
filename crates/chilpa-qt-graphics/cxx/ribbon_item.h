// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
// Spectral Ribbon (immersive shader scene, mode 4) — the QQuickRhiItem
// behind the GPU spectrogram. Block A3 of the 2026-08-15
// immersive-completion contract (spec 01 §2.4). Third hand-written C++
// item after LineBedItem/PlasmaItem (cxx/linebed_item.h carries the
// why-C++ rationale; this scene additionally needs a PERSISTENT texture
// written in sub-rects, which a ShaderEffect cannot express).
//
// QML properties are MEMBER-only (no NOTIFY): the QML wrapper
// (RibbonScene.qml) writes them on the pulse edge and calls update()
// itself — the same cadence contract as LineBedItem/PlasmaItem.
//
// `frame` is ONE QByteArray per viz tick, self-describing:
//   bytes 0..4   column (u32 LE) — playback-time column, 0..2047
//   bytes 4..12  epoch (u64 LE) — changes on track/seek, retained on every row
//   bytes 12..20 sequence (u64 LE) — increases on every spectral frame
//   bytes 20..532 the 512-band row (u8 per band, dB-scaled host-side)
// A full texture snapshot has the same header and a 512x2048-byte payload.
// Separate mailboxes keep a snapshot until synchronize even if a later row
// arrives before it. Epoch/sequence reject older queued rows after restore.
// (the pack's batching pattern: ONE property = ONE notify per tick).
//
// Registered as QML type `RibbonItem` in `app.chilpa.graphics` 1.0 (see the
// .cpp).

#pragma once

#include <QtCore/QByteArray>
#include <QtGui/QVector4D>
#include <QtQuick/QQuickRhiItem>

class RibbonItem : public QQuickRhiItem
{
    Q_OBJECT
    Q_PROPERTY(QByteArray frame READ frame WRITE setFrame)
    Q_PROPERTY(QVector4D energyHi MEMBER m_energyHi)

public:
    explicit RibbonItem(QQuickItem *parent = nullptr) : QQuickRhiItem(parent) {}

    QQuickRhiItemRenderer *createRenderer() override;

    QByteArray frame() const { return m_frame.isEmpty() ? m_snapshot : m_frame; }
    void setFrame(const QByteArray &frame);

    QByteArray m_frame;
    QByteArray m_snapshot;
    QVector4D m_energyHi{ 0.0f, 0.0f, 0.0f, 0.0f };
};
