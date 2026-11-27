import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.folders
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The albums taken out of the library, the latest first, each with the way
// back. Their files were never touched. The list is read only while open.
Modal {
    id: dialog
    objectName: "removedAlbums"
    preferredWidth: Metrics.playlistModalWidth
    readonly property var albums: opened ? JSON.parse(RemovedAlbums.json(RemovedAlbums.rev) || "[]") : []

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Text {
            width: parent.width
            text: I18n.tr("Removed albums", I18n.rev)
            font.pointSize: Type.titleSmall
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "removedAlbumsEmpty"
            width: parent.width
            visible: dialog.albums.length === 0
            text: I18n.tr("No album has been removed.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        ListView {
            id: rows
            objectName: "removedAlbumRows"
            width: parent.width
            height: Math.min(contentHeight, Metrics.playlistModalHeight)
            visible: dialog.albums.length > 0
            clip: true
            spacing: Metrics.gap8
            model: dialog.albums
            delegate: Row {
                required property var modelData
                width: rows.width
                spacing: Metrics.gap12
                ArtTile { width: Metrics.artRecord; height: width; path: modelData.artwork_path; radius: Metrics.radiusItem }
                Column {
                    width: parent.width - Metrics.artRecord - restore.width - parent.spacing * 2 // derived: the cover, the button and the two gaps
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        width: parent.width
                        text: modelData.title
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        font.pointSize: Type.body
                        color: Theme.color.textPrimary
                    }
                    Text {
                        width: parent.width
                        text: [modelData.artist, I18n.trCount("{} track", "{} tracks", modelData.tracks, I18n.rev)].filter(part => part !== "").join(" · ")
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        font.pointSize: Type.meta
                        color: Theme.color.textSecondary
                    }
                }
                PillButton {
                    id: restore
                    objectName: "restoreAlbum"
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Bring back", I18n.rev)
                    onClicked: RemovedAlbums.restore(modelData.id)
                }
            }
        }
        PillButton {
            anchors.right: parent.right
            kind: "primary"
            text: I18n.tr("Close", I18n.rev)
            onClicked: dialog.close()
        }
    }
}
