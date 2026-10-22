import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The sidebar playlists' label: the section's options, a new playlist and,
// where it can fold, the chevron that folds it (QBZ's PLAYLISTS toolbar).
// A right click on it opens the options too.
Item {
    id: head
    property bool foldable: true
    property bool folded: false
    signal fold(bool value)
    signal options(real left, real top)
    height: Metrics.button24
    TapHandler { acceptedButtons: Qt.RightButton; onTapped: event => head.options(event.position.x, head.height) }

    Text {
        x: Metrics.navItemPadding
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("Playlists", I18n.rev)
        font.pointSize: Type.small
        font.weight: Type.semibold
        color: Theme.color.textTertiary
    }
    IconButton {
        id: foldButton
        objectName: "sidebarPlaylistsFold"
        visible: head.foldable
        anchors.right: parent.right
        width: visible ? implicitWidth : 0
        size: Metrics.button24
        glyph: head.folded ? Icons.chevronRight : Icons.chevronDown
        tip: head.folded ? I18n.tr("Expand", I18n.rev) : I18n.tr("Collapse", I18n.rev)
        onClicked: head.fold(!head.folded)
    }
    IconButton {
        id: addButton
        anchors.right: foldButton.left
        size: Metrics.button24
        glyph: Icons.plus
        tip: I18n.tr("New playlist", I18n.rev)
        onClicked: AddToPlaylist.show("[]")
    }
    IconButton {
        anchors.right: addButton.left
        size: Metrics.button24
        glyph: Icons.dots
        tip: I18n.tr("Playlist options", I18n.rev)
        onClicked: head.options(x, head.height)
    }
}
