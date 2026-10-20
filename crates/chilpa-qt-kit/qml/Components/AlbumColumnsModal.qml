import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Which columns a list of records shows: one switch each, in the order they
// stand in the row. The source column shows only where a list mixes the
// library's records with a source's.
Modal {
    id: chooser
    objectName: "albumColumns"

    property var shown: []
    signal chosen(var columns)

    readonly property var all: [
        { name: "year", text: I18n.tr("Year", I18n.rev) },
        { name: "tracks", text: I18n.tr("Tracks", I18n.rev) },
        { name: "duration", text: I18n.tr("Duration", I18n.rev) },
        { name: "genre", text: I18n.tr("Genre", I18n.rev) },
        { name: "rating", text: I18n.tr("Rating", I18n.rev) },
        { name: "source", text: I18n.tr("Source", I18n.rev) }
    ]

    Column {
        width: Metrics.sleepTimer
        spacing: Metrics.gap8

        Text {
            text: I18n.tr("Columns", I18n.rev)
            font.pointSize: Type.section
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Repeater {
            model: chooser.all
            Item {
                required property var modelData
                width: parent.width
                height: Metrics.toggleHeight + Metrics.gap8

                Text {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.text
                    font.pointSize: Type.body
                    color: Theme.color.textPrimary
                }
                Toggle {
                    objectName: "column-" + modelData.name
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    checked: chooser.shown.indexOf(modelData.name) >= 0
                    onToggled: (on) => chooser.chosen(chooser.all.map(c => c.name)
                        .filter(name => name === modelData.name ? on : chooser.shown.indexOf(name) >= 0))
                }
            }
        }
    }
}
