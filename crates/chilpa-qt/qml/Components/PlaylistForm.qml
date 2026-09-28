import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// Shared by creation and editing. Labels remain visible while typing.
Column {
    id: form
    property alias name: nameField.text
    property alias description: descriptionField.text
    property alias tags: tagsField.text
    property alias artwork: artworkField.text
    readonly property bool valid: name.trim() !== ""
    signal accepted()
    spacing: Metrics.gap8

    function focusInput() { nameField.focusInput() }
    function reset() { name = ""; description = ""; tags = ""; artwork = "" }

    Text { text: I18n.tr("Playlist name", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
    TextField {
        id: nameField
        objectName: "playlistName"
        width: parent.width
        placeholder: I18n.tr("Playlist name", I18n.rev)
        onAccepted: if (form.valid) form.accepted()
    }
    Text { text: I18n.tr("Description", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
    TextField { id: descriptionField; width: parent.width; placeholder: I18n.tr("Description", I18n.rev) }
    Text { text: I18n.tr("Tags, separated by commas", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
    TextField { id: tagsField; width: parent.width; placeholder: I18n.tr("Tags, separated by commas", I18n.rev) }
    Text { text: I18n.tr("Custom image", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
    Row {
        width: parent.width
        spacing: Metrics.gap8
        TextField { id: artworkField; width: parent.width - choose.width - parent.spacing; placeholder: I18n.tr("Custom image", I18n.rev) }
        PillButton {
            id: choose
            text: I18n.tr("Choose", I18n.rev)
            onClicked: {
                const path = Playlists.chooseArtwork()
                if (path !== "") form.artwork = path
            }
        }
    }
}
