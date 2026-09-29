import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// Where the sidebar's destinations go while the sidebar is closed: a button
// in the tab bar that opens them as a menu under it, the playlists among
// them, which lead to their manager.
Rectangle {
    id: button

    readonly property var places: [
        { screen: "home", glyph: Icons.home, text: I18n.tr("Home", I18n.rev) },
        { screen: "library", glyph: Icons.columns3, text: I18n.tr("Library", I18n.rev) },
        { screen: "artists", glyph: Icons.microphone2, text: I18n.tr("Artists", I18n.rev) },
        { screen: "albums", glyph: Icons.disc, text: I18n.tr("Albums", I18n.rev) },
        { screen: "tracks", glyph: Icons.music, text: I18n.tr("Tracks", I18n.rev) },
        { screen: "folders", glyph: Icons.folder, text: I18n.tr("Folders", I18n.rev) },
        { screen: "playlistManager", glyph: Icons.playlist, text: I18n.tr("Playlists", I18n.rev) }
    ]

    implicitWidth: label.implicitWidth + Metrics.gap10 * 2 // derived: padding on both sides
    implicitHeight: Metrics.button28
    radius: Metrics.radiusControl
    color: menu.opened ? Theme.color.stateSelected : area.containsMouse ? Theme.color.stateHover : "transparent"

    Row {
        id: label
        anchors.centerIn: parent
        spacing: Metrics.gap4

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Menu", I18n.rev)
            font.pointSize: Type.meta
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: Icons.chevronDown
            size: Metrics.iconXs
            color: Theme.color.textSecondary
        }
    }

    MouseArea {
        id: area
        cursorShape: Qt.PointingHandCursor
        anchors.fill: parent
        hoverEnabled: true
        onClicked: menu.opened ? menu.close() : menu.open()
    }

    Menu {
        id: menu
        y: button.height + Metrics.gap4
        entries: button.places.map((place) => ({
            text: place.text,
            glyph: place.glyph,
            checked: Navigation.screen === place.screen
        }))
        onChosen: (index) => Navigation.go(button.places[index].screen, 0)
    }
}
