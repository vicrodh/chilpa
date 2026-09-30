// hot-path: one per track row.
import QtQuick
import "../Theme"
import "../Primitives"

// The quality tier as one glyph's color (frontend ADR-0012). The tier comes
// from the engine; this only picks the token for it.
Icon {
    // 0 unknown, 1 lossy, 2 lossless, 3 hi-res, 4 DSD.
    property int tier: 0
    readonly property color tierColor: tier >= 3 ? Theme.color.accent : tier === 2 ? Theme.color.textSecondary : Theme.color.textTertiary

    glyph: Icons.waveSquare
    size: Metrics.iconSm
    visible: tier > 0
    color: tierColor
}
