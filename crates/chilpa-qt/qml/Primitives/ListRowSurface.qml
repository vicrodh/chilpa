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
    property color restColor: "transparent"

    color: selected ? Theme.color.rowSelected
         : emphasized ? Theme.color.rowPlaying
         : hovered ? Theme.color.stateHover
         : listMode && rowIndex % 2 === 1 ? Theme.color.stateStripe : restColor
}
