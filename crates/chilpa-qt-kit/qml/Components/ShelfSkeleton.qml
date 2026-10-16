import QtQuick
import "../Theme"

// I use the grid's card pitch and the track list's row pitch, so a module
// keeps its place while its models and images arrive.
Item {
    id: skeleton
    property bool tracks: false
    property bool round: false
    property bool oneRow: true
    property int count: Metrics.shelfLimit
    property real fixedCardWidth: 0
    property real trackRowHeight: Metrics.rowRich
    readonly property int columns: CardLayout.columns(width)
    readonly property real cardWidth: fixedCardWidth || CardLayout.cardWidth
    readonly property int shownCards: oneRow ? Math.min(count, columns) : count
    readonly property int rows: Math.max(1, Math.ceil(shownCards / columns))
    implicitHeight: tracks ? Metrics.rowHeader + count * trackRowHeight
        : rows * (cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight + Metrics.gridGap) - Metrics.gridGap + Metrics.cardRing * 2

    Repeater {
        model: skeleton.tracks ? skeleton.count : skeleton.shownCards
        Item {
            required property int index
            readonly property real art: skeleton.tracks ? skeleton.trackRowHeight === Metrics.rowRich ? Metrics.artRow : 0 : skeleton.cardWidth
            x: skeleton.tracks ? 0 : Metrics.cardRing + (index % skeleton.columns) * (skeleton.cardWidth + Metrics.gridGap)
            y: skeleton.tracks ? Metrics.rowHeader + index * skeleton.trackRowHeight
                : Metrics.cardRing + Math.floor(index / skeleton.columns) * (skeleton.cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight + Metrics.gridGap)
            width: skeleton.tracks ? skeleton.width : skeleton.cardWidth
            height: skeleton.tracks ? skeleton.trackRowHeight : skeleton.cardWidth + Metrics.cardTextGap + Metrics.cardTextHeight
            Rectangle {
                y: skeleton.tracks ? (parent.height - parent.art) / 2 : 0 // derived: center artwork in a row
                width: parent.art; height: width
                radius: skeleton.round ? width / 2 : Metrics.radiusCard // derived: a circular portrait
                color: Theme.color.stateHover
            }
            Rectangle {
                x: skeleton.tracks ? parent.art + Metrics.gap12 : 0
                y: skeleton.tracks ? Metrics.gap8 : skeleton.cardWidth + Metrics.cardTextGap
                width: parent.width * 0.65
                height: Metrics.gap12
                radius: Metrics.gap4
                color: Theme.color.stateHover
            }
            Rectangle {
                visible: !skeleton.tracks || skeleton.trackRowHeight === Metrics.rowRich
                x: skeleton.tracks ? parent.art + Metrics.gap12 : 0
                y: skeleton.tracks ? Metrics.gap24 : skeleton.cardWidth + Metrics.cardTextGap + Metrics.gap22
                width: parent.width * 0.4
                height: Metrics.gap10
                radius: Metrics.gap4
                color: Theme.color.stateHover
            }
        }
    }
}
