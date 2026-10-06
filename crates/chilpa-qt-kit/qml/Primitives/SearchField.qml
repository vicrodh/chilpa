import QtQuick
import "../Theme"

// An input with a search icon, a placeholder, a shortcut hint and a clear
// button (design/03 SearchField).
Rectangle {
    id: root

    property alias text: input.text
    property string placeholder
    property string shortcut
    readonly property alias editing: input.activeFocus

    signal accepted()
    // Up and Down, for a list of results to follow; Esc.
    signal stepped(int by)
    signal escaped()

    function focusInput() {
        input.forceActiveFocus()
        input.selectAll()
    }
    function release() {
        input.focus = false
    }

    implicitHeight: Metrics.fieldHeight
    radius: Metrics.radiusControl
    color: Theme.color.fillField
    border.width: input.activeFocus ? Metrics.focusRing : 0
    border.color: Theme.color.accent

    Icon {
        id: lens
        anchors.left: parent.left
        anchors.leftMargin: Metrics.fieldPadding
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.search
        size: Metrics.iconSm
        color: Theme.color.textTertiary
    }

    TextInput {
        id: input
        anchors.left: lens.right
        anchors.leftMargin: Metrics.gap6
        anchors.right: tail.left
        anchors.rightMargin: Metrics.gap6
        anchors.verticalCenter: parent.verticalCenter
        clip: true
        font.pointSize: Type.body
        color: Theme.color.textPrimary
        selectionColor: Theme.color.rowSelected
        onAccepted: root.accepted()
        Keys.onUpPressed: root.stepped(-1)
        Keys.onDownPressed: root.stepped(1)
        Keys.onEscapePressed: root.escaped()

        Text {
            anchors.fill: parent
            visible: input.text === ""
            text: root.placeholder
            font: input.font
            color: Theme.color.textTertiary
            elide: Text.ElideRight
        }
    }

    Item {
        id: tail
        anchors.right: parent.right
        anchors.rightMargin: Metrics.fieldPadding
        anchors.verticalCenter: parent.verticalCenter
        width: input.text === "" ? hint.implicitWidth : Metrics.iconSm
        height: parent.height

        Text {
            id: hint
            anchors.verticalCenter: parent.verticalCenter
            visible: input.text === ""
            text: root.shortcut
            font.pointSize: Type.micro
            color: Theme.color.textTertiary
        }

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            visible: input.text !== ""
            glyph: Icons.x
            size: Metrics.iconSm
            color: Theme.color.textTertiary

            MouseArea {
                cursorShape: Qt.PointingHandCursor
                anchors.fill: parent
                onClicked: input.text = ""
            }
        }
    }
}
