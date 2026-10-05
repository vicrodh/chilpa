import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

ListView {
    id: list
    property var rows: []
    property bool adding: false
    property bool busy: false
    signal add(string reference)
    signal remove(string member)
    signal move(string member, int before)
    height: Math.min(Metrics.confirmWidth, contentHeight)
    clip: true
    model: rows
    ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
    delegate: Row {
        required property var modelData
        required property int index
        width: list.width
        height: Metrics.fieldHeight + Metrics.gap8
        spacing: Metrics.gap8
        Text {
            width: parent.width - actions.width - Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            text: (parent.index + 1) + ". " + (parent.modelData.title || "") + " — " + (parent.modelData.artist || "")
            color: parent.modelData.available === false ? Theme.color.textTertiary : Theme.color.textPrimary
            font.pointSize: Type.body
            elide: Text.ElideRight
            textFormat: Text.PlainText
        }
        Row {
            id: actions
            readonly property var entry: parent.modelData
            readonly property int position: parent.index
            spacing: Metrics.gap8
            enabled: !list.busy
            PillButton { visible: list.adding; text: I18n.tr("Add", I18n.rev); onClicked: list.add(actions.entry.ref) }
            PillButton { visible: !list.adding; enabled: !!actions.entry.member && actions.position > 0; text: I18n.tr("Move up", I18n.rev); onClicked: list.move(actions.entry.member, actions.position - 1) }
            PillButton { visible: !list.adding; enabled: !!actions.entry.member && actions.position < list.rows.length - 1; text: I18n.tr("Move down", I18n.rev); onClicked: list.move(actions.entry.member, actions.position + 2) }
            PillButton { visible: !list.adding; enabled: !!actions.entry.member; text: I18n.tr("Remove", I18n.rev); onClicked: list.remove(actions.entry.member) }
        }
    }
}
