import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The sidebar's playlists (design/02 2, item 6): the label with a button to
// make one, a filter, and the list, which scrolls in whatever height is
// left. A right click offers the playlist's menu; a double click renames it.
Item {
    id: section

    PlaylistListModel {
        id: lists
        rev: Playlists.rev
        filter: filter.text
    }

    Item {
        id: head
        width: parent.width
        height: Metrics.button24

        Text {
            x: Metrics.navItemPadding
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Playlists", I18n.rev)
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.textTertiary
        }
        IconButton {
            anchors.right: parent.right
            size: Metrics.button24
            glyph: Icons.plus
            tip: I18n.tr("New playlist", I18n.rev)
            onClicked: Playlists.create()
        }
    }

    TextField {
        id: filter
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap6
        width: parent.width
        height: Metrics.playlistFilter
        placeholder: I18n.tr("Filter playlists", I18n.rev)
        Keys.onEscapePressed: text = ""
    }

    ListView {
        id: view
        anchors.top: filter.bottom
        anchors.topMargin: Metrics.gap6
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: lists
        ScrollBar.vertical: ScrollBar {}

        delegate: PlaylistRow {
            id: row
            width: ListView.view.width
            active: Navigation.screen === "playlist" && Navigation.routeId === playlistId
            onOpened: Navigation.go("playlist", playlistId)
            onMenu: (left, top) => {
                const at = row.mapToItem(section, left, top)
                actions.row = row
                actions.show(playlistId, name, at.x, at.y)
            }
            onRenamed: (text) => Playlists.rename(playlistId, text)
        }
    }

    Text {
        visible: lists.count === 0 && filter.text === ""
        anchors.top: filter.bottom
        anchors.topMargin: Metrics.gap10
        x: Metrics.navItemPadding
        width: parent.width - x
        wrapMode: Text.Wrap
        text: I18n.tr("No playlists yet.", I18n.rev)
        font.pointSize: Type.small
        color: Theme.color.textTertiary
    }

    PlaylistMenu {
        id: actions
        property PlaylistRow row
        onRename: if (row) row.editing = true
    }
}
