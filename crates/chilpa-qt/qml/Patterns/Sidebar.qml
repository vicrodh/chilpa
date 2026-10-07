import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The floating left panel (design/02 2): search, the main destinations and
// the playlists. Collapsed, a column of icons.
Rectangle {
    id: side

    // Its column of icons. The frame says so while a drag shows it.
    property var expandedSources: ({})
    property bool collapsed: SettingsModel.sidebarMode === "collapsed"
    // The plugins that asked for an entry of their own, once approved.
    readonly property var sourceView: JSON.parse(Sources.snapshot || "{}")
    readonly property var panels: sourceView.panels || []

    Component.onCompleted: Sources.refresh()
    Timer { interval: Motion.poll; repeat: true; running: side.sourceView.starting === true; onTriggered: Sources.refresh() }
    Connections {
        target: Navigation
        function onScreenChanged() { Sources.refresh() }
    }
    // The host is free again each time the catalog finishes asking it.
    Connections { target: Library; function onRemoteRevChanged() { Sources.refresh() } function onRevChanged() { Sources.refresh() } }

    radius: Metrics.radiusPanel
    color: Theme.color.surfaceSidebar

    Edge {}

    Column {
        anchors.fill: parent
        anchors.topMargin: Metrics.sidebarPadV
        anchors.leftMargin: Metrics.sidebarPadH
        anchors.rightMargin: Metrics.sidebarPadH
        spacing: Metrics.sidebarGap

        // Beside the sidebar, whatever its width (design/02 8).
        SearchBox {
            id: searchBox
            visible: !side.collapsed
            width: parent.width
            active: SettingsModel.sidebarMode !== "closed"
            compact: side.collapsed
            flyoutX: side.width - Metrics.sidebarPadH + Metrics.gap8 // derived: past the sidebar's edge
            flyoutY: -Metrics.sidebarPadV // derived: level with the sidebar's top
            room: side.height
        }

        IconButton {
            visible: side.collapsed
            x: (parent.width - width) / 2 // derived: centered
            tip: I18n.tr("Search", I18n.rev)
            glyph: Icons.search
            onClicked: searchBox.focusSearch()
        }

        Flickable {
            objectName: "sidebarNavigation"
            width: parent.width
            // Reserve two playlist rows below navigation when it grows.
            height: Math.min(contentHeight, Math.max(0, parent.height - y - Metrics.navItemHeight * 2 - Metrics.sidebarGap)) // derived: two rows and their gap
            contentWidth: width
            contentHeight: navigation.height
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap8;}

            Column {
                id: navigation
                width: parent.width

                Repeater {
                    model: [
                        { screen: "home", glyph: Icons.home, text: I18n.tr("Home", I18n.rev) },
                        { screen: "library", glyph: Icons.columns3, text: I18n.tr("Library", I18n.rev) },
                        { screen: "artists", glyph: SourceGlyphs.musicArtist, family: SourceGlyphs.font, text: I18n.tr("Artists", I18n.rev) },
                        { screen: "albums", glyph: Icons.disc, text: I18n.tr("Albums", I18n.rev) },
                        { screen: "tracks", glyph: Icons.music, text: I18n.tr("Tracks", I18n.rev) },
                        { screen: "folders", glyph: Icons.folder, text: I18n.tr("Folders", I18n.rev) },
                        { screen: "openMedia", glyph: Icons.folderOpen, text: I18n.tr("Open media", I18n.rev) }
                    ].concat(side.panels.map(p => ({ screen: "source", key: p.id, glyph: SourceGlyphs.of(p.icon) || Icons[p.icon] || Icons.letterX, family: SourceGlyphs.of(p.icon) ? SourceGlyphs.font : "", image: p.icon_image || "", tint: p.icon_tint || false, text: p.name, music: p.music === true, children: p.children || [] })))

                    Column {
                        id: group
                        required property var modelData
                        required property int index
                        width: parent.width

                        Text {
                            visible: group.index === 7 && !side.collapsed
                            height: visible ? Metrics.navItemHeight : 0
                            x: Metrics.navItemPadding
                            text: I18n.tr("Streaming", I18n.rev)
                            font.pointSize: Type.small; font.weight: Type.semibold
                            color: Theme.color.textTertiary; verticalAlignment: Text.AlignVCenter
                        }
                        NavItem {
                            objectName: group.modelData.key ? "sourceParent" : "navigationParent"
                            width: parent.width
                            expandable: !!group.modelData.key
                            expanded: side.expandedSources[group.modelData.key] === true
                            onExpansionClicked: { const copy = Object.assign({}, side.expandedSources); copy[group.modelData.key] = !expanded; side.expandedSources = copy }
                            text: group.modelData.text
                            glyph: group.modelData.glyph; iconFamily: group.modelData.family || ""
                            tintIcon: group.modelData.tint || false
                            iconSource: group.modelData.image || ""
                            collapsed: side.collapsed
                            active: Navigation.screen === group.modelData.screen && (!group.modelData.key || Navigation.routeKey === group.modelData.key)
                            onClicked: {
                                if (!group.modelData.key) Navigation.go(group.modelData.screen, 0)
                                else {
                                    // A tree starts over from its root; music opens its own page.
                                    if (!group.modelData.music) Sources.command("home", group.modelData.key, "")
                                    Navigation.goKey(group.modelData.screen, group.modelData.key)
                                }
                            }
                            onMiddleClicked: group.modelData.key ? clicked() : SettingsModel.middleClickTabs ? Navigation.open(group.modelData.screen, 0) : Navigation.go(group.modelData.screen, 0)
                        }
                        Repeater {
                            model: group.modelData.children || []
                            NavItem {
                                required property var modelData
                                objectName: "sourceChild"
                                visible: !side.collapsed && side.expandedSources[group.modelData.key] === true
                                x: Metrics.navItemPadding
                                width: parent.width - x
                                text: modelData.title
                                glyph: Icons[modelData.icon] || Icons.folder
                                active: modelData.route ? Navigation.routeKey === modelData.route && (modelData.route !== group.modelData.key || Navigation.screen === "source")
                                    : side.sourceView.navigation_id === modelData.id
                                onClicked: {
                                    if (modelData.route) { Navigation.goKey("source", modelData.route); return }
                                    Sources.command("child", group.modelData.key, modelData.id)
                                    Navigation.goKey("source", group.modelData.key)
                                }
                                onMiddleClicked: clicked()
                            }
                        }
                    }
                }
            }
        }

        // Whatever height is left, down to the sidebar's bottom padding.
        SidebarPlaylists {
            visible: !side.collapsed
            width: parent.width
            height: Math.max(0, parent.height - y - Metrics.sidebarPadV) // derived: the rest of the column
        }
    }
}
