pragma Singleton
import QtQuick

// I size every album cover from the Library viewport. Narrower sections
// fit fewer of those cards instead of introducing a smaller cover size.
QtObject {
    property real viewportWidth: 0
    readonly property real referenceWidth: Math.max(0, viewportWidth - Metrics.contentX * 2 - Metrics.cardRing * 2)
    readonly property int referenceColumns: Math.max(Metrics.gridMinColumns, Math.floor((referenceWidth + Metrics.gridGap) / Metrics.cardMinWidth))
    readonly property real cardWidth: viewportWidth > 0
        ? (referenceWidth - (referenceColumns - 1) * Metrics.gridGap) / referenceColumns : Metrics.cardMinWidth
    readonly property real cardHeight: cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight
    function columns(width) {
        return Math.max(1, Math.floor((Math.max(0, width - Metrics.cardRing * 2) + Metrics.gridGap) / (cardWidth + Metrics.gridGap)))
    }
}
