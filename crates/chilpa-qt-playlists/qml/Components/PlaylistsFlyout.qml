import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The playlists while the sidebar is closed (QBZ's SidebarPlaylistsPopup,
// HeaderBar.qml:627-900): the sidebar's own list with its filter, up to six
// rows in view, then Import and Manage playlists. Opening a playlist closes
// it, and closing it empties the filter.
Popup {
    id: flyout
    PopupInput { surface: flyout }

    // A row under the list: its icon and what it does, as the sidebar's rows.
    component Way: Rectangle {
        id: way
        property string text
        property string glyph
        signal chosen()
        width: parent ? parent.width : 0
        height: Metrics.navItemHeight
        radius: Metrics.radiusCard
        color: area.containsMouse ? Theme.color.stateHover : "transparent"
        Row {
            x: Metrics.navItemPadding
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap8
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: way.glyph; iconFamily: Lucide.font; size: Metrics.iconSm; color: Theme.color.textSecondary }
            Text { anchors.verticalCenter: parent.verticalCenter; text: way.text; font.pointSize: Type.body; color: Theme.color.textSecondary }
        }
        MouseArea { id: area; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: way.chosen() }
    }
    objectName: "playlistsFlyout"
    // A click outside closes it and is nothing else.
    modal: true
    dim: false
    width: Metrics.sidebarWidth
    padding: Metrics.gap8
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnReleaseOutside
    onClosed: lists.clearFilter()

    background: Rectangle {
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised
        border.width: 1
        border.color: Theme.color.strokeHairline
    }

    contentItem: Column {
        spacing: Metrics.gap6
        SidebarPlaylists {
            id: lists
            width: parent.width
            // As tall as its rows, six at most, as QBZ's (HeaderBar.qml:741).
            height: Metrics.button24 + Metrics.playlistFilter + Metrics.gap6 * 2 + Metrics.playlistRowHeight * Math.max(1, Math.min(6, rows.length)) // derived: the head, the filter and up to six rows
            collapsible: false
            onPlaylistOpened: flyout.close()
        }
        Rectangle { width: parent.width; height: 1; color: Theme.color.strokeHairline }
        Way {
            objectName: "playlistsFlyoutImport"
            text: I18n.tr("Import playlist", I18n.rev)
            glyph: Lucide.importGlyph
            onChosen: { flyout.close(); Navigation.go("playlistImport", 0) }
        }
        Way {
            objectName: "playlistsFlyoutManage"
            text: I18n.tr("Manage playlists", I18n.rev)
            glyph: Lucide.libraryBig
            onChosen: { flyout.close(); Navigation.go("playlistManager", 0) }
        }
    }
}
