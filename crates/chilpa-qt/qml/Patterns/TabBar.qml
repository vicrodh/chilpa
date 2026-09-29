import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The strip above everything (design/02 3): history and the sidebar's
// button at the window's left, after the room macOS wants for its own
// controls; then the tabs; density and, outside macOS,
// the window's controls. Its empty space moves the window, and a double
// click maximizes it.
Item {
    id: bar

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
        // The sidebar's destinations, while it is closed.
        NavMenu {
            visible: SettingsModel.sidebarMode === "closed"
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
            id: more
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Menu", I18n.rev)
            glyph: Icons.menu2
            onClicked: mainMenu.open()

            Menu {
                id: mainMenu
                y: more.height + Metrics.gap4
                x: more.width - width
                entries: [
                    { text: I18n.tr("Settings", I18n.rev), hint: "Ctrl+," },
                    { text: I18n.tr("Library folders", I18n.rev) },
                    { separator: true },
                    { text: I18n.tr("Quit", I18n.rev), hint: "Ctrl+Q" }
                ]
                onChosen: (index) => index === 0 ? Navigation.openSection("")
                    : index === 1 ? Navigation.go("libraryFolders", 0)
                    : Qt.quit()
            }
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
