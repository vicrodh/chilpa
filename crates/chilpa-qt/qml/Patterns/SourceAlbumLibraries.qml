import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: libraries
    property bool active: false
    property bool tracks: false
    property var model: active ? JSON.parse((tracks ? Sources.trackLibraries(Library.remoteRev + Library.rev + Sources.revision) : Sources.albumLibraries(Library.remoteRev + Library.rev + Sources.revision)) || "[]") : []
    visible: model.length > 0
    spacing: Metrics.gap8
    Text {
        text: I18n.tr("Streaming libraries", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }
    Text {
        width: parent.width
        text: libraries.tracks ? I18n.tr("Show saved tracks in Tracks", I18n.rev) : I18n.tr("Show saved releases in Albums", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textSecondary
        wrapMode: Text.WordWrap
    }
    Repeater {
        model: libraries.model
        Item {
            required property var modelData
            width: libraries.width
            height: Metrics.sourceFilterRow
            SourceIcon {
                id: icon
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                sourceInfo: ({icon: modelData.icon, label: modelData.name})
            }
            Text {
                anchors.left: icon.right
                anchors.leftMargin: Metrics.gap10
                anchors.right: toggle.left
                anchors.rightMargin: Metrics.gap10
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.name
                elide: Text.ElideRight
                font.pointSize: Type.body
                color: Theme.color.textPrimary
            }
            Toggle {
                id: toggle
                objectName: "albumLibraryToggle-" + modelData.id
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                checked: modelData.enabled === true
                onToggled: on => Sources.command(libraries.tracks ? "track_library" : "album_library", modelData.id, on ? "true" : "false")
            }
        }
    }
}
