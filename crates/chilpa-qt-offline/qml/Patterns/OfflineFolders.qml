import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"

// Where offline music goes: the folder every server inherits, then each
// server with what it keeps and its own folder when it has one. A new
// folder takes the next downloads; what is saved stays where it is.
Flickable {
    id: folders
    property var doc: ({})
    property string said: ""
    signal rootSaved()
    signal serverFolderChosen(int id, string folder)
    contentHeight: column.height + Metrics.contentBottom
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.gap4 }

    Column {
        id: column
        width: folders.width - Metrics.gap12
        spacing: Metrics.gap24
        OfflineRootFolder {
            objectName: "offlineRootFolder"
            width: parent.width
            onSaved: folders.rootSaved()
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: folders.said
            textFormat: Text.PlainText; wrapMode: Text.WordWrap
            font.pointSize: Type.small; color: Theme.color.statusNegative
        }
        Repeater {
            model: folders.doc.servers || []
            delegate: Column {
                id: server
                required property var modelData
                width: column.width
                spacing: Metrics.gap8
                Row {
                    spacing: Metrics.gap8
                    SourceIcon { anchors.verticalCenter: parent.verticalCenter; sourceInfo: ({icon: server.modelData.icon}) }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, server.width - Metrics.iconMd - Metrics.gap8) // derived: beside its icon
                        text: server.modelData.name
                        textFormat: Text.PlainText; elide: Text.ElideRight
                        font.pointSize: Type.bodyLarge; font.weight: Type.semibold; color: Theme.color.textPrimary
                    }
                }
                Text {
                    width: parent.width
                    text: I18n.trCount("{} track", "{} tracks", server.modelData.files || 0, I18n.rev) + " · " + (server.modelData.size || "")
                    textFormat: Text.PlainText; wrapMode: Text.WordWrap
                    font.pointSize: Type.small; color: Theme.color.textTertiary
                }
                FolderChoice {
                    objectName: "offlineServerFolder_" + server.modelData.id
                    width: parent.width
                    title: I18n.tr("Offline folder for this server", I18n.rev)
                    description: I18n.tr("Use the global offline folder or choose a separate destination.", I18n.rev)
                    defaultLabel: I18n.tr("Global offline folder", I18n.rev)
                    defaultFolder: server.modelData.inherited || ""
                    folder: server.modelData.folder || ""
                    onChosen: selected => folders.serverFolderChosen(server.modelData.id, selected)
                }
            }
        }
    }
}
