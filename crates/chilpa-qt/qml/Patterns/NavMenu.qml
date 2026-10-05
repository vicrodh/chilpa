import QtQuick
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../Theme"
import "../Primitives"

// Where the sidebar's destinations go while the sidebar is closed: a button
// in the tab bar that opens them as a menu under it, the playlists among
// them, which lead to their manager.
Rectangle {
    id: button

    readonly property var panels: JSON.parse(Sources.snapshot || "{}").panels || []
    readonly property var places: [
        { screen: "home", glyph: Icons.home, text: I18n.tr("Home", I18n.rev) },
        { screen: "library", glyph: Icons.columns3, text: I18n.tr("Library", I18n.rev) },
        { screen: "artists", glyph: SourceGlyphs.musicArtist, family: SourceGlyphs.font, text: I18n.tr("Artists", I18n.rev) },
        { screen: "albums", glyph: Icons.disc, text: I18n.tr("Albums", I18n.rev) },
        { screen: "tracks", glyph: Icons.music, text: I18n.tr("Tracks", I18n.rev) },
        { screen: "folders", glyph: Icons.folder, text: I18n.tr("Folders", I18n.rev) },
        { screen: "openMedia", glyph: Icons.folderOpen, text: I18n.tr("Open media", I18n.rev) },
        { screen: "playlistManager", glyph: Icons.playlist, text: I18n.tr("Playlists", I18n.rev) }
    ].concat(panels.map(p => ({ screen: "source", key: p.id, text: p.name,
        glyph: SourceGlyphs.of(p.icon) || Icons[p.icon] || Icons.server,
        family: SourceGlyphs.of(p.icon) ? SourceGlyphs.font : "", music: p.music === true })))

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
        objectName: "navigationMenu"
        onAboutToShow: Sources.refresh()
        y: button.height + Metrics.gap4
        entries: button.places.map((place) => ({
            text: place.text,
            glyph: place.glyph, family: place.family || "",
            checked: Navigation.screen === place.screen && (!place.key || Navigation.routeKey === place.key)
        }))
        onChosen: (index) => {
            const place = button.places[index]
            if (!place.key) Navigation.go(place.screen, 0)
            else {
                if (!place.music) Sources.command("home", place.key, "")
                Navigation.goKey(place.screen, place.key)
            }
        }
    }
}
