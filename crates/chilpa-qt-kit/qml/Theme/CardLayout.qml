pragma Singleton
import QtQuick

// I keep covers the same size while pages and sidebars resize.
QtObject {
    property real viewportWidth: 0
    readonly property real cardWidth: Metrics.albumCardWidth
    readonly property real cardHeight: cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight
    function columns(width) {
        return Math.max(1, Math.floor((Math.max(0, width - Metrics.cardRing * 2) + Metrics.gridGap) / (cardWidth + Metrics.gridGap)))
    }
}
