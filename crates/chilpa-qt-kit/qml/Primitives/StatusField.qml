import QtQuick
import "../Theme"

// A state pill whose menu contains the actions valid in that state. An
// optional account link sits beside it and opens once when it appears.
Row {
    id: root

    property string state
    // The color of its light: "positive", "warning" or "negative". Empty
    // shows none.
    property string light
    property string link
    property string openText
    property var options: []
    property var values: []

    signal picked(string value)

    spacing: Metrics.gap8

    Rectangle {
        anchors.verticalCenter: parent.verticalCenter
        visible: root.light !== ""
        width: Metrics.gap8
        height: width
        radius: width / 2 // derived: a circle
        color: root.light === "positive" ? Theme.color.statusPositive : root.light === "negative" ? Theme.color.statusNegative : Theme.color.statusWarning
    }

    PillButton {
        id: stateButton
        anchors.verticalCenter: parent.verticalCenter
        text: root.state
        glyph: Icons.chevronDown
        onClicked: actions.open()

        Menu {
            id: actions
            y: stateButton.height + Metrics.gap4
            x: stateButton.width - width
            entries: root.options.map((text) => ({ text: text }))
            onChosen: (index) => root.picked(root.values[index])
        }
    }

    PillButton {
        anchors.verticalCenter: parent.verticalCenter
        visible: root.link !== ""
        text: root.openText
        glyph: Icons.arrowUpRight
        onClicked: Qt.openUrlExternally(root.link)
        Component.onCompleted: if (visible) Qt.openUrlExternally(root.link)
    }
}
