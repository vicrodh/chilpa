import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/discovery/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Where the sidebar's destinations go while the sidebar is closed: a button
// in the tab bar that opens them as a menu under it, the playlists among
// them, which lead to their manager.
Rectangle {
    id: button
    property var openSource: function(action, id, text) { Navigation.goKey("source", id) }

    readonly property var panels: JSON.parse(Sources.snapshot || "{}").panels || []
    LibraryDestinations { id: destinations }
    readonly property var places: destinations.entries
    .concat([{ screen: "playlistManager", glyph: Icons.playlist, text: I18n.tr("Playlists", I18n.rev) }])
    .concat(panels.map(p => ({ screen: "source", key: p.id, text: p.name,
        glyph: SourceGlyphs.of(p.icon) || Icons[p.icon] || Icons.server,
        sourceIcon: p.icon_image || "",
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
            leadingSourceIcon: place.sourceIcon || "",
            checked: Navigation.screen === place.screen && (!place.key || Navigation.routeKey === place.key)
        }))
        onChosen: (index) => {
            const place = button.places[index]
            if (!place.key) Navigation.go(place.screen, 0)
            else {
                if (!place.music) button.openSource("home", place.key, "")
                else Navigation.goKey(place.screen, place.key)
            }
        }
    }
}
