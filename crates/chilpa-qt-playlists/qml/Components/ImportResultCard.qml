import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What one destination got: how many songs went in and what was left out,
// what went wrong, and the way to open each playlist it made.
Rectangle {
    id: card
    property var output: ({})
    property string name
    property var sourceInfo: ({})
    signal opened(var part)
    implicitHeight: body.implicitHeight + Metrics.gap16 * 2 // derived: padding above and below
    radius: Metrics.radiusCard
    color: Theme.color.fillField
    SourceIcon { id: mark; x: Metrics.gap16; y: Metrics.gap16; width: Metrics.iconMd; height: width; sourceInfo: card.sourceInfo }
    Column {
        id: body
        x: mark.x + mark.width + Metrics.gap12; y: Metrics.gap16
        width: card.width - x - Metrics.gap16
        spacing: Metrics.gap8
        Text {
            objectName: "importResultName"
            width: parent.width; text: card.name; textFormat: Text.PlainText; elide: Text.ElideRight
            font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary
        }
        Text {
            objectName: "importResultCounts"
            width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText; font.pointSize: Type.small; color: Theme.color.textSecondary
            text: [I18n.trCount("{} song added", "{} songs added", card.output.written || 0, I18n.rev),
                (card.output.unmatched || 0) > 0 ? I18n.trCount("{} not found", "{} not found", card.output.unmatched, I18n.rev) : "",
                (card.output.duplicates || 0) > 0 ? I18n.trCount("{} duplicate", "{} duplicates", card.output.duplicates, I18n.rev) : ""].filter(Boolean).join(" · ")
        }
        Text {
            objectName: "importResultError"
            width: parent.width; visible: text !== ""; wrapMode: Text.Wrap; textFormat: Text.PlainText
            font.pointSize: Type.small; color: Theme.color.statusNegative; text: card.output.error || ""
        }
        Repeater {
            model: card.output.parts || []
            Item {
                required property var modelData
                width: body.width; height: open.height
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - open.width - Metrics.gap12
                    text: modelData.title || ""; textFormat: Text.PlainText; elide: Text.ElideRight
                    font.pointSize: Type.body; color: Theme.color.textPrimary
                }
                PillButton { id: open; objectName: "importOpen"; anchors.right: parent.right; text: I18n.tr("Open playlist", I18n.rev); onClicked: card.opened(modelData) }
            }
        }
    }
}
