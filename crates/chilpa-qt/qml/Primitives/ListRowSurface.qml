import QtQuick
import "../Theme"

// Shared visual contract for list rows. Cards and grids set listMode false.
Rectangle {
    required property int index
    property bool listMode: true
    property bool hovered: false
    property bool selected: false
    property bool emphasized: false
    property color restColor: "transparent"

    color: selected ? Theme.color.rowSelected
         : emphasized ? Theme.color.rowPlaying
         : hovered ? Theme.color.stateHover
         : listMode && index % 2 === 1 ? Theme.color.stateStripe : restColor
}
