import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// A bordered box of the options that belong to the one control above it,
// titled with what that control chose (QBZ's `SettingsFieldset.qml`). Its
// chevron folds the rows; whoever holds it keeps the folded state.
Rectangle {
    id: box

    property string title
    property bool folded: false
    default property alias content: body.data

    signal foldAsked()

    implicitHeight: header.height + (folded ? 0 : body.implicitHeight + Metrics.gap12)
    radius: Metrics.radiusPanel
    color: "transparent"
    border.width: 1
    border.color: Theme.color.strokeFrame

    Rectangle {
        id: header
        objectName: "optionsHeader"
        x: 1
        y: 1
        width: parent.width - 2 // derived: inside the border
        height: Editor.boxHeader
        radius: Metrics.radiusPanel
        color: headerArea.containsMouse ? Theme.color.stateHover : "transparent"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: box.title
        Accessible.onPressAction: box.foldAsked()
        Keys.onPressed: (event) => {
            if (!event.isAutoRepeat && (event.key === Qt.Key_Space || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)) {
                box.foldAsked()
                event.accepted = true
            }
        }
        FocusRing {}

        Row {
            anchors.left: parent.left
            anchors.leftMargin: Metrics.gap10
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                glyph: box.folded ? Icons.chevronRight : Icons.chevronDown
                size: Metrics.iconMd
                color: Theme.color.textSecondary
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: box.title
                color: Theme.color.textPrimary
                font.pointSize: Type.body
                font.weight: Type.semibold
            }
        }
        MouseArea {
            id: headerArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onPressed: header.forceActiveFocus()
            onClicked: box.foldAsked()
        }
    }

    Column {
        id: body
        visible: !box.folded
        x: Metrics.gap12
        y: header.y + header.height
        width: box.width - Metrics.gap12 * 2 // derived: the side insets
        spacing: Metrics.gap4
    }
}
