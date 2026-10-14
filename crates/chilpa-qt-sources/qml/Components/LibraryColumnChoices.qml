import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: choices
    property var selected: null
    signal toggled(string column)
    visible: selected !== null
    spacing: Metrics.gap4
    Text {
        text: I18n.tr("Columns", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }
    Repeater {
        model: [
            {column:"year", label:I18n.tr("Year", I18n.rev)},
            {column:"genre", label:I18n.tr("Genre", I18n.rev)},
            {column:"label", label:I18n.tr("Label", I18n.rev)},
            {column:"composer", label:I18n.tr("Composer", I18n.rev)},
            {column:"rating", label:I18n.tr("Rating", I18n.rev)},
            {column:"artist", label:I18n.tr("Artist", I18n.rev)},
            {column:"album", label:I18n.tr("Album", I18n.rev)}
        ]
        Item {
            required property var modelData
            width: choices.width
            height: Metrics.button28
            Text {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.label
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
            Toggle {
                objectName: "paneToggle-" + modelData.column
                compact: true
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: choices.selected !== null && choices.selected.indexOf(modelData.column) >= 0
                onToggled: choices.toggled(modelData.column)
            }
        }
    }
}
