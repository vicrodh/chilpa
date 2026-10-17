import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: dialog
    objectName: "playlistEditDialog"
    property int playlistId: 0
    property int folderId: 0
    property string notice: ""
    width: Math.max(Metrics.modalInsets, Math.min(Metrics.playlistModalWidth, parent.width - Metrics.modalInset))
    height: Math.max(Metrics.modalInsets, Math.min(body.implicitHeight + Metrics.modalInsets, parent.height - Metrics.modalInset))

    function show(id) {
        playlistId = id
        folderId = Playlists.folderId(id, Playlists.rev)
        form.name = Playlists.name(id, Playlists.rev)
        form.description = Playlists.description(id, Playlists.rev)
        form.tags = Playlists.tags(id, Playlists.rev)
        form.artwork = Playlists.artwork(id, Playlists.rev)
        notice = ""
        open()
        Qt.callLater(form.focusInput)
    }
    function save() {
        if (!form.valid) return
        if (Playlists.update(playlistId, form.name, form.description, form.artwork, form.tags, folderId)) close()
        else notice = I18n.tr("Could not save the playlist. Please try again.", I18n.rev)
    }

    Flickable {
        anchors.fill: parent
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}
        Column {
            id: body
            width: parent.width
            spacing: Metrics.gap12
            Row {
                width: parent.width
                Text {
                    width: parent.width - closeButton.width
                    text: I18n.tr("Edit playlist", I18n.rev)
                    font.pointSize: Type.titleSmall
                    font.weight: Type.bold
                    color: Theme.color.textPrimary
                }
                IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
            }
            PlaylistForm { id: form; width: parent.width; onAccepted: dialog.save() }
            Text {
                visible: dialog.notice !== ""
                width: parent.width
                text: dialog.notice
                wrapMode: Text.Wrap
                font.pointSize: Type.small
                color: Theme.color.statusWarning
            }
            Row {
                anchors.right: parent.right
                spacing: Metrics.gap8
                PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: dialog.close() }
                PillButton { kind: "primary"; text: I18n.tr("Save", I18n.rev); enabled: form.valid; onClicked: dialog.save() }
            }
        }
    }
}
