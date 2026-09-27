import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The floating left panel (design/02 2): the main destinations and the
// playlists. Collapsed, a column of icons.
Rectangle {
    id: side

    // Its column of icons. The frame says so while a drag shows it.
    property bool collapsed: SettingsModel.sidebarMode === "collapsed"

    radius: Metrics.radiusPanel
    color: Theme.color.surfaceSidebar

    Edge {}

    Column {
        anchors.fill: parent
        anchors.topMargin: Metrics.sidebarPadV
        anchors.leftMargin: Metrics.sidebarPadH
        anchors.rightMargin: Metrics.sidebarPadH
        spacing: Metrics.sidebarGap

        // The search is at the top of every screen now, not here.
        IconButton {
            id: toggle
            x: side.collapsed ? (parent.width - width) / 2 : parent.width - width // derived: centered when collapsed, at the edge when open
            tip: side.collapsed ? I18n.tr("Expand sidebar", I18n.rev) : I18n.tr("Collapse sidebar", I18n.rev)
            glyph: side.collapsed ? Icons.layoutSidebarLeftExpand : Icons.layoutSidebarLeftCollapse
            onClicked: SettingsModel.sidebarMode = side.collapsed ? "open" : "collapsed"
        }

        Column {
            width: parent.width

            Repeater {
                model: [
                    { screen: "home", glyph: Icons.home, text: I18n.tr("Home", I18n.rev) },
                    { screen: "library", glyph: Icons.columns3, text: I18n.tr("Library", I18n.rev) },
                    { screen: "artists", glyph: Icons.microphone2, text: I18n.tr("Artists", I18n.rev) },
                    { screen: "albums", glyph: Icons.disc, text: I18n.tr("Albums", I18n.rev) },
                    { screen: "folders", glyph: Icons.folder, text: I18n.tr("Folders", I18n.rev) }
                ]

                NavItem {
                    required property var modelData
                    width: parent.width
                    text: modelData.text
                    glyph: modelData.glyph
                    collapsed: side.collapsed
                    active: Navigation.screen === modelData.screen
                    onClicked: Navigation.go(modelData.screen, 0)
                    onMiddleClicked: SettingsModel.middleClickTabs ? Navigation.open(modelData.screen, 0) : Navigation.go(modelData.screen, 0)
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
