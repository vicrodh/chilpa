import QtQuick
import "../Theme"

// Shared visual contract for music-entity list rows. Grids set listMode false.
Rectangle {
    // Keep this separate from a view delegate's implicit `index`.  A reusable
    // surface must never make its parent delegate impossible to instantiate.
    property int rowIndex: 0
    property bool listMode: true
    property bool hovered: false
    property bool selected: false
    property bool emphasized: false
    // The row of the track that plays, marked at its start in accent, as in
    // QBZ. Off when the moving bars are the mark instead.
    property bool edge: false
    property color restColor: "transparent"

    color: selected ? Theme.color.rowSelected
         : emphasized ? Theme.color.rowPlaying
         : hovered ? Theme.color.stateHover
         : listMode && rowIndex % 2 === 1 ? Theme.color.stateStripe : restColor

    Rectangle {
        visible: parent.edge
        x: Metrics.gap2
        y: Metrics.playingEdgeInset
        width: Metrics.playingEdge
        height: parent.height - 2 * Metrics.playingEdgeInset // derived: short at both ends
        radius: width / 2 // derived: rounded ends
        color: Theme.color.accent
    }
}
