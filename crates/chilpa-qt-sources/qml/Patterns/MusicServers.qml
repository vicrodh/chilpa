import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "qrc:/qt/qml/app/chilpa/offline/qml/Components"
import "qrc:/qt/qml/app/chilpa/server/playback/qml/Components"

Item {
    id: table
    property alias model: list.model
    signal addRequested()
    signal actionRequested(string key, string action)

    Column {
        id: introduction
        width: parent.width
        spacing: Metrics.gap12
        Item {
            width: parent.width
            height: Metrics.settingsRow
            Text {
                anchors.left: parent.left
                anchors.right: add.left
                anchors.rightMargin: Metrics.gap16
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Add a new server", I18n.rev)
                font.pointSize: Type.body
                font.weight: Type.medium
                color: Theme.color.textPrimary
            }
            PillButton {
                id: add
                objectName: "addMusicServer"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("Add…", I18n.rev)
                onClicked: table.addRequested()
            }
        }
        Text {
            width: parent.width
            text: I18n.tr("Connect Jellyfin, Navidrome or another OpenSubsonic server, Plex, or a DLNA / UPnP media server to add its music to your library.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        OfflineRootFolder {
            objectName: "globalOfflineFolder"
            width: parent.width
        }
        // The page with every download, its limit and each server's folder.
        PillButton {
            objectName: "offlineDownloads"
            text: I18n.tr("Offline music", I18n.rev)
            glyph: Icons.download
            onClicked: Offline.requested("downloads", "[]")
        }
        ServerTableRow { width: parent.width; heading: true }
    }
    ListView {
        id: list
        objectName: "musicServerList"
        anchors.top: introduction.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        bottomMargin: Metrics.contentBottom // the last rows scroll clear of a floating player
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.pageGutter }
        delegate: ServerTableRow {
            required property var model
            readonly property var entry: model.modelData || model
            readonly property bool isServer: (entry.key || "").startsWith("sources.server/")
            visible: isServer
            width: ListView.view.width
            height: isServer ? implicitHeight : 0
            name: entry.title || ""
            detail: entry.description || ""
            status: entry.action || ""
            server: isServer ? JSON.parse(entry.value || "{}") : ({})
            options: entry.options || []
            values: entry.optionValues || []
            // Its playback settings open here, and keep each change at once.
            onActionRequested: value => value === "playback" ? playback.show(Number(entry.key.split("/")[1])) : table.actionRequested(entry.key, value)
        }
    }
    ServerPlaybackDialog {
        id: playback
        objectName: "serverPlayback"
        // The server's line says when it is told nothing.
        onChanged: if (table.model && typeof table.model.refresh === "function") table.model.refresh()
    }
}
