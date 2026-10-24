import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "qrc:/qt/qml/app/chilpa/about/qml/Components"
import "qrc:/qt/qml/app/chilpa/link/qml/Components" as LinkUi
import "qrc:/qt/qml/app/chilpa/wizard/qml/Components" as WizardUi
import "qrc:/qt/qml/app/chilpa/playlists/qml/Components" as Lists
import app.chilpa.about
import app.chilpa.controls

// The strip above everything (design/02 3): history and the sidebar's
// button at the window's left, after the room macOS wants for its own
// controls; then the tabs; density and, outside macOS,
// the window's controls. Its empty space moves the window, and a double
// click maximizes it.
Item {
    id: bar
    property var openSource: function(action, id, text) { Navigation.goKey("source", id) }
    // The menu's Quit, which the window's controls ask about first.
    signal quitAsked()

    function menuChosen(act) {
        if (act === "settings") Navigation.openSection("")
        else if (act === "folders") Navigation.go("libraryFolders", 0)
        else if (act === "stats") Navigation.go("stats", 0)
        else if (act === "about") about.open()
        else if (["appUpdates", "pluginUpdates", "whatsNew"].includes(act)) updateInbox.open(act)
        else if (act === "documentation" || act === "issue") Qt.openUrlExternally(About.link(act))
        else if (act === "close") bar.Window.window.close()
        else if (act === "quit") bar.quitAsked()
        else Hotkeys.ask(act)
    }

    // What the system's own controls take of the bar's left, on a Mac.
    property real controlsRoom: 0

    readonly property bool mac: Qt.platform.os === "osx"
    // The sidebar goes from open to collapsed to closed and back to open.
    readonly property var turns: ({
        open: { next: "collapsed", glyph: Icons.layoutSidebarLeftCollapse, tip: I18n.tr("Collapse sidebar", I18n.rev) },
        collapsed: { next: "closed", glyph: Icons.layoutSidebarLeftCollapse, tip: I18n.tr("Hide sidebar", I18n.rev) },
        closed: { next: "open", glyph: Icons.layoutSidebarLeftExpand, tip: I18n.tr("Show the sidebar", I18n.rev) }
    })
    readonly property var turn: turns[SettingsModel.sidebarMode] || turns.open

    implicitHeight: Metrics.tabBarHeight

    // no cursor: the bar's empty space moves the window
    DragHandler {
        target: null
        onActiveChanged: if (active) bar.Window.window.startSystemMove()
    }
    // no cursor: the bar's empty space moves the window
    TapHandler {
        onDoubleTapped: bar.Window.window.visibility === Window.Maximized ? bar.Window.window.showNormal() : bar.Window.window.showMaximized()
    }

    Row {
        id: lead
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.topMargin: Metrics.tabBarTop
        spacing: Metrics.gap4

        // The system's three controls float here.
        Item {
            visible: bar.mac
            width: bar.controlsRoom
            height: Metrics.button28
        }
        Rectangle {
            visible: bar.mac
            anchors.verticalCenter: parent.verticalCenter
            width: 1
            height: Metrics.dividerHeight
            color: Theme.color.strokeHairline
        }
        IconButton {
            tip: I18n.tr("Back", I18n.rev)
            glyph: Icons.chevronLeft
            tone: "primary"
            enabled: Navigation.canBack
            onClicked: Navigation.back()
        }
        IconButton {
            tip: I18n.tr("Forward", I18n.rev)
            glyph: Icons.chevronRight
            tone: "primary"
            enabled: Navigation.canForward
            onClicked: Navigation.forward()
        }
        IconButton {
            tip: bar.turn.tip
            glyph: bar.turn.glyph
            onClicked: SettingsModel.sidebarMode = bar.turn.next
        }
        // The sidebar's destinations and its playlists, while it is closed
        // (QBZ's HeaderBar.qml:627).
        NavMenu {
            openSource: bar.openSource
            visible: SettingsModel.sidebarMode === "closed"
        }
        IconButton {
            objectName: "playlistsFlyoutButton"
            visible: SettingsModel.sidebarMode === "closed"
            tip: I18n.tr("Playlists", I18n.rev)
            glyph: Icons.playlist
            checked: playlistsFlyout.opened
            onClicked: playlistsFlyout.opened ? playlistsFlyout.close() : playlistsFlyout.open()
            Lists.PlaylistsFlyout { id: playlistsFlyout; y: parent.height + Metrics.gap4 }
        }
        Rectangle {
            visible: SettingsModel.sidebarMode === "closed"
            anchors.verticalCenter: parent.verticalCenter
            width: 1
            height: Metrics.dividerHeight
            color: Theme.color.strokeHairline
        }
    }

    TabStrip {
        anchors.left: parent.left
        anchors.leftMargin: lead.width + Metrics.gap8 // derived: right after the buttons
        anchors.top: parent.top
        anchors.topMargin: Metrics.tabBarTop
        room: bar.width - anchors.leftMargin - tools.width - Metrics.gap12 // derived: what the two ends leave
    }

    Row {
        id: tools
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.topMargin: Metrics.tabBarTop
        spacing: Metrics.gap8

        SegmentedControl {
            options: [
                { glyph: Icons.list, tip: I18n.tr("Compact rows", I18n.rev) },
                { glyph: Icons.layoutList, tip: I18n.tr("Rich rows", I18n.rev) }
            ]
            current: SettingsModel.density === "rich" ? 1 : 0
            onActivated: (index) => SettingsModel.density = index === 1 ? "rich" : "compact"
        }

        // With the sidebar closed, its search lives here, results below it
        // and ending where the field ends.
        SearchBox {
            visible: SettingsModel.sidebarMode === "closed"
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.screenSearch
            active: visible
            flyoutX: width - Metrics.searchFlyout // derived: its right edge on the field's
            flyoutY: height + Metrics.gap10
            room: bar.Window.window ? bar.Window.window.height - Metrics.tabBarHeight - Metrics.spaceWindow : 0 // derived: under the tab bar
        }

        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Open ListenList", I18n.rev)
            glyph: Lucide.squarePlay
            iconFamily: Lucide.font
            onClicked: Navigation.go("listenlist", 0)
        }

        IconButton {
            id: more
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Menu", I18n.rev)
            glyph: Icons.menu2
            onClicked: mainMenu.open()

            UpdatesIndicator {
                id: updateInbox
                anchors.top: parent.top
                anchors.right: parent.right
                review: function(target) { target === "libraryFolders" ? Navigation.go(target, 0) : Navigation.openSection(target || "plugins") }
            }
            CascadingMenu {
                id: mainMenu
                y: more.height + Metrics.gap4
                x: more.width - width
                // QBZ's app menu (HeaderBar.qml:1428-1667), Chilpa's own pages
                // after Settings; the hints are the keymap's, not literals.
                entries: [
                    { act: "settings", text: I18n.tr("Settings", I18n.rev), glyph: Lucide.settings2, family: Lucide.font, hint: Hotkeys.shortcut("nav.settings") },
                    { act: "ui.openLink", text: I18n.tr("Open music link", I18n.rev), glyph: Lucide.link, family: Lucide.font, hint: Hotkeys.shortcut("ui.openLink") },
                    { act: "folders", text: I18n.tr("Library folders", I18n.rev), glyph: Lucide.folder, family: Lucide.font },
                    { act: "stats", text: I18n.tr("Listening stats", I18n.rev), glyph: Lucide.chartColumn, family: Lucide.font },
                    { act: "ui.showShortcuts", text: I18n.tr("Keyboard shortcuts", I18n.rev), glyph: Lucide.keyboard, family: Lucide.font, hint: Hotkeys.shortcut("ui.showShortcuts") },
                    { act: "documentation", text: I18n.tr("Documentation", I18n.rev), glyph: Lucide.bookOpen, family: Lucide.font },
                    { act: "issue", text: I18n.tr("Report an issue", I18n.rev), glyph: Lucide.bug, family: Lucide.font },
                    { text: I18n.tr("Updates", I18n.rev), submenu: true, mark: updateInbox.unread ? "•" : "", children: [
                        { act: "appUpdates", text: I18n.tr("Chilpa", I18n.rev), detail: updateInbox.appState.version || "", enabled: updateInbox.appState.supported === true },
                        { act: "pluginUpdates", text: I18n.tr("Plugins", I18n.rev), detail: String(updateInbox.pluginState.unread || "") }
                    ] },
                    { act: "whatsNew", text: I18n.tr("What's new", I18n.rev), glyph: Icons.star },
                    { act: "about", text: I18n.tr("About Chilpa", I18n.rev), glyph: Lucide.info, family: Lucide.font },
                    { separator: true },
                    // Close is the window's (the tray keeps Chilpa, or it asks
                    // to quit); Quit is the app's, as in QBZ's menu.
                    { act: "close", text: I18n.tr("Close", I18n.rev), glyph: Lucide.x, family: Lucide.font },
                    { act: "quit", text: I18n.tr("Quit", I18n.rev), glyph: Lucide.power, family: Lucide.font, hint: Hotkeys.shortcut("nav.quit") }
                ]
                onActionChosen: entry => bar.menuChosen(entry.act)
            }
            AboutModal { id: about }
            LinkUi.LinkModal {}
            WizardUi.HifiWizardModal {}
        }

        Rectangle {
            visible: !bar.mac
            anchors.verticalCenter: parent.verticalCenter
            width: 1
            height: Metrics.dividerHeight
            color: Theme.color.strokeHairline
        }

        Repeater {
            model: bar.mac ? [] : [
                { glyph: Icons.minus, tip: I18n.tr("Minimize", I18n.rev), act: "min" },
                { glyph: Icons.square, tip: I18n.tr("Maximize", I18n.rev), act: "max" },
                { glyph: Icons.x, tip: I18n.tr("Close", I18n.rev), act: "close" }
            ]

            IconButton {
                required property var modelData
                anchors.verticalCenter: parent.verticalCenter
                tip: modelData.tip
                glyph: modelData.glyph
                size: Metrics.windowControl
                iconSize: Metrics.iconXs
                round: true
                onClicked: {
                    const w = bar.Window.window
                    if (modelData.act === "min")
                        w.showMinimized()
                    else if (modelData.act === "max")
                        w.visibility === Window.Maximized ? w.showNormal() : w.showMaximized()
                    else
                        w.close()
                }
            }
        }
    }
}
