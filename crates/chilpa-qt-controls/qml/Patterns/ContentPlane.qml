import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The clip follows the panels. Album grids and track sheets only see the
// new width once the panels have settled.
Item {
    id: plane

    required property bool moving
    required property real leftInset
    required property real rightInset
    required property real topInset
    required property real bottomInset
    default property alias content: layout.data
    property bool cornered: false
    property real heldWidth: -1
    onMovingChanged: heldWidth = moving ? layout.width : -1

    x: leftInset
    y: topInset
    width: Math.max(0, parent.width - x - rightInset)
    height: parent.height - y - bottomInset
    clip: true

    Item {
        id: layout
        width: plane.heldWidth >= 0 ? plane.heldWidth : plane.width
        height: plane.height
    }
    PlaneCorners {
        visible: plane.cornered
        width: plane.width
    }
}
