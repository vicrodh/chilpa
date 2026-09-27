import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// Metadata editor for a local playlist.
Modal {
    id: dialog

    property int playlistId: 0
    property int folderId: 0

    width: Metrics.playlistModalWidth

    function show(id) {
        playlistId = id
        folderId = Playlists.folderId(id, Playlists.rev)
        nameField.text = Playlists.name(id, Playlists.rev)
        descriptionField.text = Playlists.description(id, Playlists.rev)
        tagsField.text = Playlists.tags(id, Playlists.rev)
        artworkField.text = Playlists.artwork(id, Playlists.rev)
        open()
        Qt.callLater(nameField.focusInput)
    }

    Column {
        spacing: Metrics.gap12
        Text { text: I18n.tr("Edit playlist", I18n.rev); font.pointSize: Type.titleSmall; font.weight: Type.bold; color: Theme.color.textPrimary }
        TextField { id: nameField; width: parent.width; placeholder: I18n.tr("Playlist name", I18n.rev) }
        TextField { id: descriptionField; width: parent.width; placeholder: I18n.tr("Description", I18n.rev) }
        TextField { id: tagsField; width: parent.width; placeholder: I18n.tr("Tags, separated by commas", I18n.rev) }
        Row {
            width: parent.width
            spacing: Metrics.gap8
            TextField { id: artworkField; width: parent.width - choose.width - parent.spacing; placeholder: I18n.tr("Custom image", I18n.rev) }
            PillButton { id: choose; text: I18n.tr("Choose", I18n.rev); onClicked: artworkField.text = Playlists.chooseArtwork() }
        }
        Row {
            spacing: Metrics.gap8
            PillButton { text: I18n.tr("Cancel", I18n.rev); onClicked: dialog.close() }
            PillButton {
                kind: "primary"
                text: I18n.tr("Save", I18n.rev)
                enabled: nameField.text.trim() !== ""
                onClicked: {
                    if (Playlists.update(dialog.playlistId, nameField.text,
                                         descriptionField.text, artworkField.text,
                                         tagsField.text, dialog.folderId))
                        dialog.close()
                }
            }
        }
    }
}
