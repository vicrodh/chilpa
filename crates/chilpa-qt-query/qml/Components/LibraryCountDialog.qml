import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Modal {
    id: dialog
    objectName: "libraryCountDialog"
    property string total: Library.summary
    property var counts: JSON.parse(Library.countDetails || "{}")

    Loader {
        id: body
        width: parent.width
        active: dialog.visible
        sourceComponent: Column {
            width: body.width
            spacing: Metrics.gap12

            Text {
                width: parent.width
                text: I18n.tr("Library counts", I18n.rev)
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            CountRow { name: I18n.tr("All sources", I18n.rev); value: dialog.total }
            CountRow { name: I18n.tr("Local", I18n.rev); value: String(dialog.counts.local ?? 0) }
            CountRow {
                name: I18n.trArgs("Self-hosted ({})", [String(dialog.counts.servers ?? 0)], I18n.rev)
                value: String(dialog.counts.serverTracks ?? 0)
            }
            Repeater {
                model: dialog.counts.plugins || []
                CountRow { required property var modelData; name: modelData[0]; value: String(modelData[1]) }
            }
            CountRow { name: I18n.tr("Showing", I18n.rev); value: dialog.counts.showing || "" }
            Text {
                width: parent.width
                text: I18n.tr("Song counts include the tracks loaded into your library. Albums can appear before their track lists are loaded.", I18n.rev)
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            PillButton {
                anchors.right: parent.right
                text: I18n.tr("Close", I18n.rev)
                onClicked: dialog.close()
            }
        }
    }

    component CountRow: Row {
        id: row
        required property string name
        required property string value
        width: parent.width
        spacing: Metrics.gap12

        Text {
            width: Math.max(0, row.width - tally.implicitWidth - row.spacing)
            text: row.name
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.pointSize: Type.body
            color: Theme.color.textPrimary
        }
        Text {
            id: tally
            text: row.value
            textFormat: Text.PlainText
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
    }
}
