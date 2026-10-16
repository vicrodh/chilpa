import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// What the About window says under its head, in QBZ's order: what Chilpa
// is, the links, the build's facts in two columns, and the thanks.
Column {
    id: body
    property var doc: ({})
    readonly property var facts: [
        [{ label: I18n.tr("Version", I18n.rev), value: body.doc.version || "" }, { label: I18n.tr("Build", I18n.rev), value: body.doc.build || "" }],
        [{ label: I18n.tr("Platform", I18n.rev), value: body.doc.platform || "" }]
    ]

    Text {
        width: parent.width
        text: I18n.tr("Chilpatl is Nahuatl for hornet. Chilpa is a music player for the desktop, where local files, self-hosted servers and plugins are all just sources.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    Item { width: 1; height: Metrics.gap22 }
    Row {
        spacing: Metrics.gap12
        Repeater {
            model: body.doc.links || []
            PillButton {
                required property var modelData
                objectName: "aboutLink"
                text: modelData.label
                glyph: Icons.externalLink
                onClicked: Qt.openUrlExternally(modelData.url)
            }
        }
    }
    Item { width: 1; height: Metrics.gap24 }
    Text {
        text: I18n.tr("Build info", I18n.rev)
        font.pointSize: Type.micro
        font.weight: Type.semibold
        font.letterSpacing: Type.labelTracking
        color: Theme.color.textTertiary
    }
    Item { width: 1; height: Metrics.gap12 }
    Row {
        width: parent.width
        spacing: Metrics.gap16
        Repeater {
            model: body.facts
            Column {
                required property var modelData
                width: (body.width - Metrics.gap16) / 2 // derived: two columns and the gap between them
                spacing: Metrics.gap6
                Repeater {
                    model: modelData
                    Row {
                        required property var modelData
                        spacing: Metrics.gap12
                        Text { width: Frame.factLabel; text: modelData.label; font.pointSize: Type.body; color: Theme.color.textTertiary }
                        Text { objectName: "aboutFact"; text: modelData.value; font.pointSize: Type.body; color: Theme.color.textPrimary }
                    }
                }
            }
        }
    }
    Item { width: 1; height: Metrics.gap24 }
    UpdatesPanel { width: parent.width }
    Text {
        text: I18n.tr("Acknowledgments", I18n.rev)
        font.pointSize: Type.micro
        font.weight: Type.semibold
        font.letterSpacing: Type.labelTracking
        color: Theme.color.textTertiary
    }
    Item { width: 1; height: Metrics.gap12 }
    Text {
        width: parent.width
        text: I18n.tr("Built with Rust and Qt, and powered by Symphonia and Lofty. With thanks to MusicBrainz, Last.fm, LRCLIB, Tabler Icons, Lucide and Inter.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.meta
        color: Theme.color.textSecondary
    }
}
