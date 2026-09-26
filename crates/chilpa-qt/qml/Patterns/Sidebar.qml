import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The floating left panel (design/02 2): search, the main destinations and
// the playlists. Collapsed, a column of icons.
Rectangle {
    id: side

    // Its column of icons. The frame says so while a drag shows it.
    property bool collapsed: SettingsModel.sidebarMode === "collapsed"

    radius: Metrics.radiusPanel
    color: Theme.color.surfaceSidebar

    Edge {}

    SearchModel { id: search }

    Shortcut {
        sequence: "Ctrl+K"
        onActivated: {
            if (side.collapsed) {
                flyout.ownInput = true
                flyout.open()
            } else {
                field.focusInput()
            }
        }
    }

    Column {
        anchors.fill: parent
        anchors.topMargin: Metrics.sidebarPadV
        anchors.leftMargin: Metrics.sidebarPadH
        anchors.rightMargin: Metrics.sidebarPadH
        spacing: Metrics.sidebarGap

        Row {
            width: parent.width
            spacing: Metrics.gap6

            SearchField {
                id: field
                visible: !side.collapsed
                width: parent.width - toggle.width - parent.spacing
                placeholder: I18n.tr("Search", I18n.rev)
                shortcut: Qt.platform.os === "osx" ? "⌘K" : "Ctrl K"
                onTextChanged: {
                    search.text = text
                    text !== "" ? flyout.open() : flyout.close()
                }
                onEditingChanged: if (editing && text !== "") flyout.open()
                onStepped: (by) => flyout.step(by)
                onAccepted: flyout.chooseCurrent()
                onEscaped: {
                    flyout.close()
                    release()
                }

                // Beside the sidebar, whatever its width (design/02 8).
                SearchFlyout {
                    id: flyout
                    search: search
                    x: side.width - Metrics.sidebarPadH + Metrics.gap8 // derived: past the sidebar's edge
                    y: Metrics.searchTop - Metrics.spaceWindow - Metrics.sidebarPadV // derived: from the field to the window's top
                    room: side.height
                    onClosed: if (!side.collapsed) field.release()
                }
            }

            IconButton {
                id: toggle
                x: side.collapsed ? (parent.width - width) / 2 : 0 // derived: centered when collapsed
                tip: side.collapsed ? I18n.tr("Expand sidebar", I18n.rev) : I18n.tr("Collapse sidebar", I18n.rev)
                glyph: side.collapsed ? Icons.layoutSidebarLeftExpand : Icons.layoutSidebarLeftCollapse
                onClicked: SettingsModel.sidebarMode = side.collapsed ? "open" : "collapsed"
            }
        }

        IconButton {
            visible: side.collapsed
            x: (parent.width - width) / 2 // derived: centered
            tip: I18n.tr("Search", I18n.rev)
            glyph: Icons.search
            onClicked: {
                flyout.ownInput = true
                flyout.open()
            }
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
                    onMiddleClicked: Navigation.open(modelData.screen, 0)
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
