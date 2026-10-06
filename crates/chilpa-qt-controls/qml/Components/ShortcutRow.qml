import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "../Theme"

// One action and its key. On the sheet it is read; in the editor the key
// cap is pressed to give the action another key, and while that goes on the
// row says which key was tried and who already has it.
Rectangle {
    id: row

    // The action as the sheet's document has it: id, label, shortcut and
    // whether the listener changed it.
    property var entry: ({})
    property bool editing: false
    property bool recording: false
    property string tried
    property string takenBy

    signal record()
    signal reset()

    implicitHeight: Math.max(Sizes.keyRow, end.implicitHeight + Metrics.gap8)
    radius: Metrics.radiusCard
    color: editing ? "transparent" : Theme.color.fillField

    Text {
        anchors.left: parent.left
        anchors.leftMargin: row.editing ? 0 : Metrics.gap12
        anchors.right: end.left
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: parent.verticalCenter
        text: row.entry.label || ""
        elide: Text.ElideRight
        font.pointSize: Type.body
        color: Theme.color.textPrimary
    }
    Column {
        id: end
        anchors.right: parent.right
        anchors.rightMargin: row.editing ? 0 : Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4

        Row {
            anchors.right: parent.right
            spacing: Metrics.gap8

            Keycap {
                objectName: "keycap"
                text: row.recording ? (row.tried || I18n.tr("Press keys…", I18n.rev)) : (row.entry.shortcut || "")
                button: row.editing
                recording: row.recording
                modified: row.editing && row.entry.modified === true
                conflict: row.recording && row.takenBy !== ""
                onClicked: row.record()
            }
            IconButton {
                visible: row.editing && row.entry.modified === true
                tip: I18n.tr("Restore default", I18n.rev)
                glyph: Icons.refresh
                onClicked: row.reset()
            }
        }
        Text {
            objectName: "conflict"
            visible: row.recording && row.takenBy !== ""
            anchors.right: parent.right
            text: I18n.trArgs("Already used by {}", [row.takenBy], I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.statusNegative
        }
    }
}
