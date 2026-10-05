import QtQuick
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Primitives"

Rectangle {
    id: panel
    property var entry: ({})
    signal closed()
    signal played()
    signal saved()
    signal opened()
    implicitHeight: body.height + Metrics.gap24; radius: Metrics.radiusPanel; color: Theme.color.fillField
    Column {
        id: body; x: Metrics.gap12; y: Metrics.gap12; width: parent.width - Metrics.gap24; spacing: Metrics.gap12
        Text { width: parent.width; text: panel.entry.title || ""; wrapMode: Text.Wrap; font.pointSize: Type.section; font.weight: Type.heading; color: Theme.color.textPrimary }
        Text { width: parent.width; text: panel.entry.playable === false ? panel.entry.whyNot : panel.entry.subtitle || ""; wrapMode: Text.Wrap; color: Theme.color.textSecondary; font.pointSize: Type.body }
        Text { width: parent.width; visible: text !== ""; text: panel.entry.description || ""; maximumLineCount: 3; wrapMode: Text.Wrap; elide: Text.ElideRight; color: Theme.color.textSecondary; font.pointSize: Type.body; textFormat: Text.PlainText }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            PillButton { text: I18n.tr("Play", I18n.rev); kind: "primary"; enabled: panel.entry.playable !== false; onClicked: panel.played() }
            PillButton { text: panel.entry.saved ? I18n.tr("Saved", I18n.rev) : I18n.tr("Save", I18n.rev); onClicked: panel.saved() }
            PillButton { text: I18n.tr("Full details", I18n.rev); onClicked: panel.opened() }
            PillButton { text: I18n.tr("Close", I18n.rev); onClicked: panel.closed() }
        }
    }
}
