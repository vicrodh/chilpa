import QtQuick

// What a panel drawn over the window puts under its own content: every
// press and every turn of the wheel that nothing in the panel took stops
// here, and is not also a click or a scroll on what the panel covers.
// no cursor: it takes clicks so that nothing under it does
MouseArea {
    anchors.fill: parent
    acceptedButtons: Qt.AllButtons
    onWheel: (wheel) => wheel.accepted = true
}
