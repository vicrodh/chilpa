import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Which columns a list of records shows: one switch each, in the order they
// stand in the row. The source column shows only where a list mixes the
// library's records with a source's. The design's width at least, and as
// wide as the longest name needs beside its switch; in a window narrower
// than that, a name wraps and its row grows.
Modal {
    id: chooser
    objectName: "albumColumns"
    preferredWidth: Math.max(Metrics.sleepTimer, namesWide + Metrics.gap12 + Metrics.toggleWidth) + Metrics.modalInsets

    property var shown: []
    property var options: []
    signal chosen(var columns)

    // The longest name on one line, as the rows set them.
    readonly property real namesWide: {
        let most = 0
        for (const row of list.children) most = Math.max(most, row.wants || 0)
        return most
    }

    readonly property var all: options.length > 0 ? options : [
        { name: "year", text: I18n.tr("Year", I18n.rev) },
        { name: "tracks", text: I18n.tr("Tracks", I18n.rev) },
        { name: "duration", text: I18n.tr("Duration", I18n.rev) },
        { name: "genre", text: I18n.tr("Genre", I18n.rev) },
        { name: "rate", text: I18n.tr("Sample rate", I18n.rev) },
        { name: "bits", text: I18n.tr("Bit depth", I18n.rev) },
        { name: "rating", text: I18n.tr("Rating", I18n.rev) },
        { name: "source", text: I18n.tr("Source", I18n.rev) }
    ]

    Column {
        id: list
        width: parent.width
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
                readonly property real wants: label.implicitWidth
                width: parent.width
                height: Math.max(switcher.height, label.height) + Metrics.gap8

                Text {
                    id: label
                    anchors.left: parent.left
                    anchors.right: switcher.left
                    anchors.rightMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.text
                    wrapMode: Text.WordWrap
                    font.pointSize: Type.body
                    color: Theme.color.textPrimary
                }
                Toggle {
                    id: switcher
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
