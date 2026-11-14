import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Rectangle {
    id: row
    property var entry: ({})
    property bool busy: false
    property bool selectable: true
    property string hardlinkPolicy: ""
    signal selectionChosen(bool selected)
    function shown(value) { return value != null && typeof value !== "string" && value.length !== undefined ? Array.from(value).join(" · ") : String(value ?? "") }
    implicitHeight: body.implicitHeight + Metrics.gap12 * 2 // derived: row padding
    radius: Metrics.radiusControl
    color: entry.selected ? Theme.color.stateSelected : Theme.color.surfacePanel
    Column {
        id: body
        x: Metrics.gap12; y: Metrics.gap12
        width: parent.width - Metrics.gap12 * 2 // derived: row padding
        spacing: Metrics.gap6
        Item {
            width: parent.width; height: Math.max(choice.height, name.implicitHeight)
            Toggle {
                id: choice; compact: true; visible:row.selectable; enabled: !!row.entry.key && !row.busy
                checked: row.entry.selected === true
                Accessible.name: (row.entry.path || "")
                onToggled: selected => row.selectionChosen(selected)
            }
            Text {
                id: name
                anchors.left: choice.visible ? choice.right : parent.left; anchors.leftMargin: choice.visible ? Metrics.gap12 : 0; anchors.right: parent.right
                text: row.entry.path || I18n.tr("Loading…", I18n.rev)
                textFormat: Text.PlainText; elide: Text.ElideMiddle
                font.pointSize: Type.body; color: Theme.color.textPrimary
            }
        }
        Text {
            width: parent.width
            text: [row.entry.artist,row.entry.album,row.entry.title].filter(Boolean).join(" · ")
            textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.meta; color: Theme.color.textSecondary
        }
        Repeater {
            model: row.entry.changes || []
            Column {
                required property var modelData
                width: body.width; spacing: Metrics.gap2
                Text { width:parent.width; text:modelData.label || modelData.field; textFormat:Text.PlainText; font.pointSize:Type.meta; font.weight:Type.semibold; color:Theme.color.textPrimary }
                Text { width:parent.width; text: I18n.tr("Before", I18n.rev) + ": " + row.shown(modelData.before); textFormat:Text.PlainText; wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
                Text { width:parent.width; text: I18n.tr("After", I18n.rev) + ": " + row.shown(modelData.after); textFormat:Text.PlainText; wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.textPrimary }
            }
        }
        Text {
            width: parent.width; visible: !!row.entry.error || !!row.entry.statusLabel
            text: row.entry.error || row.entry.statusLabel || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap
            color: row.entry.error ? Theme.color.statusNegative : Theme.color.textSecondary; font.pointSize: Type.meta
        }
        Text {
            width:parent.width; visible:row.entry.fileWritten === true
            text:I18n.tr("The file tags have been written.", I18n.rev); textFormat:Text.PlainText; wrapMode:Text.Wrap
            font.pointSize:Type.meta; color:Theme.color.textSecondary
        }
        Text {
            width:parent.width; visible:row.entry.hardlinked === true && row.hardlinkPolicy !== ""
            text:row.hardlinkPolicy; textFormat:Text.PlainText; wrapMode:Text.Wrap
            font.pointSize:Type.meta; color:Theme.color.textSecondary
        }
    }
}
