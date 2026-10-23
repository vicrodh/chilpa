import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The results page's tabs and, at their end on a tab of songs, the way to
// choose several of them, as the Tracks screen has it.
Item {
    id: bar

    property var tabs: []
    property string tab
    property bool selectable: false
    property bool selecting: false
    // Whether any song is chosen yet: until one is, the row says how.
    property bool chosenAny: false
    signal picked(string id)
    signal selectToggled()

    implicitHeight: row.height

    Row {
        id: row
        objectName: "searchTabs"
        spacing: Metrics.gap24
        Repeater {
            model: bar.tabs
            Column {
                required property var modelData
                readonly property bool active: bar.tab === modelData.id
                objectName: "searchTab_" + modelData.id
                spacing: Metrics.gap4
                Text {
                    text: parent.modelData.text
                    font.pointSize: Type.body; font.weight: parent.active ? Type.bold : Type.regular
                    color: parent.active ? Theme.color.textPrimary : Theme.color.textSecondary
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: bar.picked(parent.parent.modelData.id) }
                }
                Rectangle { visible: parent.active; width: parent.width; height: Metrics.gap2; radius: height / 2; color: Theme.color.accent } // derived: a rounded underline
            }
        }
    }
    Row {
        anchors.right: parent.right
        anchors.verticalCenter: row.verticalCenter
        spacing: Metrics.gap8
        visible: bar.selectable
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, bar.width - row.width - select.width - Metrics.gap24 - Metrics.gap8) // derived: what the tabs leave
            visible: bar.selecting && !bar.chosenAny
            text: I18n.tr("Click tracks to select them; Shift selects a range.", I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textTertiary
        }
        IconButton {
            id: select
            objectName: "searchSelectSongs"
            tip: I18n.tr("Select tracks", I18n.rev)
            glyph: Icons.checkbox
            round: true
            checked: bar.selecting
            onClicked: bar.selectToggled()
        }
    }
}
