import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Rectangle {
    id: fact
    property string heading
    property string value
    property string note
    width: parent ? parent.width : 0
    implicitHeight: lines.implicitHeight + Metrics.gap12 * 2 // derived: card insets
    height: implicitHeight
    radius: Metrics.radiusItem
    color: Theme.color.surfacePanel
    Column {
        id: lines
        anchors.fill: parent
        anchors.margins: Metrics.gap12
        spacing: Metrics.gap6
        Text {
            width: parent.width; text: fact.heading; textFormat: Text.PlainText
            wrapMode: Text.Wrap; font.pointSize: Type.meta; color: Theme.color.textTertiary
        }
        Text {
            width: parent.width; text: fact.value; textFormat: Text.PlainText
            wrapMode: Text.Wrap; font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary
        }
        Text {
            width: parent.width; text: fact.note; visible: text !== ""; textFormat: Text.PlainText
            wrapMode: Text.Wrap; font.pointSize: Type.meta; color: Theme.color.textSecondary
        }
    }
}
