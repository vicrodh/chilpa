import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Rectangle {
    id: row
    property var entry: ({})
    property bool busy: false
    property bool included: false
    signal selectionChosen(bool selected)
    signal opened()
    signal lookupChosen()
    signal groupingChosen()
    implicitHeight: body.implicitHeight + Metrics.gap12 * 2 // derived: row padding
    radius: Metrics.radiusControl; color:Theme.color.surfacePanel
    Column {
        id:body; x:Metrics.gap12; y:Metrics.gap12
        width:parent.width - Metrics.gap12 * 2; spacing:Metrics.gap8 // derived: row padding
        Text { width:parent.width; text:row.entry.title || I18n.tr("Loading…",I18n.rev); textFormat:Text.PlainText; elide:Text.ElideRight; font.pointSize:Type.row; color:Theme.color.textPrimary }
        Text { width:parent.width; text:row.entry.artist || ""; textFormat:Text.PlainText; elide:Text.ElideRight; font.pointSize:Type.body; color:Theme.color.textSecondary }
        Text { width:parent.width; text:row.entry.location || ""; textFormat:Text.PlainText; elide:Text.ElideMiddle; font.pointSize:Type.meta; color:Theme.color.textTertiary }
        Flow {
            width:parent.width; spacing:Metrics.gap8
            PillButton { text:I18n.tr("Files",I18n.rev); enabled:!row.busy && !!row.entry.key; onClicked:row.opened() }
            PillButton { text:I18n.tr("Look up with beets",I18n.rev); enabled:!row.busy && row.entry.lookup?.allowed === true; onClicked:row.lookupChosen() }
            PillButton { text:row.included ? I18n.tr("Remove from disc plan",I18n.rev) : I18n.tr("Use in disc plan",I18n.rev); enabled:!row.busy && !!row.entry.key; onClicked:row.groupingChosen() }
        }
        Flow {
            width:parent.width; spacing:Metrics.gap8
            Toggle { compact:true; checked:(row.entry.selected || 0) > 0; enabled:!row.busy && !!row.entry.key; Accessible.name:I18n.tr("Select album files",I18n.rev); onToggled:value=>row.selectionChosen(value) }
            Text { text:I18n.trArgs("{} of {} files selected",[row.entry.selected || 0,row.entry.tracks || 0],I18n.rev); font.pointSize:Type.meta; color:Theme.color.textSecondary }
        }
        Text { width:parent.width; visible:(row.entry.conflicts || []).length > 0; text:(row.entry.conflicts || []).map(value=>value.message).join("\n"); textFormat:Text.PlainText; wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.statusWarning }
        Text { width:parent.width; visible:row.entry.lookup?.allowed === false; text:row.entry.lookup?.reason || ""; textFormat:Text.PlainText; wrapMode:Text.Wrap; font.pointSize:Type.meta; color:Theme.color.textSecondary }
    }
}
