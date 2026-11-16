import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

ListRowSurface {
    id: row
    property var change: ({})
    property bool busy: false
    signal choose(string key, bool selected)
    implicitHeight: Math.max(Metrics.metadataChangeHeight + (change.invalid ? Metrics.metadataChangeInvalid : 0), body.implicitHeight + Metrics.gap8 * 2) // derived: content and padding on both sides
    selected: change.selected === true
    function shown(value, mixed) { return mixed ? I18n.tr("Mixed", I18n.rev) : value || I18n.tr("Empty", I18n.rev) }
    Column {
        id: body
        anchors.fill: parent; anchors.margins: Metrics.gap8
        spacing: Metrics.gap4
        CircleCheck {
            id: check
            objectName: "metadataChange:" + (row.change.key || "")
            width: parent.width
            text: (row.change.title ? row.change.title + " · " : "") + (row.change.label || "")
            checked: row.change.selected === true
            enabled: !row.busy && !(row.change.invalid || "")
            activeFocusOnTab: enabled
            contentItem: Text {
                text:check.text; textFormat:Text.PlainText; leftPadding:check.indicator.width + check.spacing
                verticalAlignment:Text.AlignVCenter; wrapMode:Text.WrapAtWordBoundaryOrAnywhere; maximumLineCount:2; elide:Text.ElideRight
                color:Theme.color.textPrimary; font.pointSize:Type.body
            }
            Accessible.description: I18n.tr("Current", I18n.rev) + ": " + row.shown(row.change.current,row.change.current_mixed)
                + ". " + I18n.tr("Proposed", I18n.rev) + ": " + row.shown(row.change.proposed,row.change.proposed_mixed)
            onToggled: row.choose(row.change.key, checked)
            Keys.onReturnPressed: row.choose(row.change.key, !checked)
            Keys.onEnterPressed: row.choose(row.change.key, !checked)
        }
        Row {
            width: parent.width
            spacing: Metrics.gap12
            Repeater {
                model: [{label:I18n.tr("Current",I18n.rev),value:row.shown(row.change.current,row.change.current_mixed)},
                    {label:I18n.tr("Proposed",I18n.rev),value:row.shown(row.change.proposed,row.change.proposed_mixed)}]
                Column {
                    required property var modelData
                    width: (parent.width - parent.spacing) / 2 // derived: current and proposed values
                    spacing: Metrics.gap2
                    Text { width:parent.width; text:modelData.label; textFormat:Text.PlainText; color:Theme.color.textTertiary; font.pointSize:Type.small }
                    Text { width:parent.width; text:modelData.value; textFormat:Text.PlainText; wrapMode:Text.WrapAtWordBoundaryOrAnywhere; maximumLineCount:2; elide:Text.ElideRight; color:Theme.color.textPrimary; font.pointSize:Type.meta }
                }
            }
        }
        Text { width:parent.width; visible:text !== ""; text:row.change.invalid || ""; textFormat:Text.PlainText; wrapMode:Text.WordWrap; maximumLineCount:2; elide:Text.ElideRight; color:Theme.color.statusWarning; font.pointSize:Type.small }
    }
    HoverHandler { id:pointer }
    ToolTip.visible: pointer.hovered
    ToolTip.text: shown(change.current,change.current_mixed) + "\n→\n" + shown(change.proposed,change.proposed_mixed) + (change.invalid ? "\n" + change.invalid : "")
}
