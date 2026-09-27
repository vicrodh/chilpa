import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The strip above the content (design/02 3): with the sidebar closed, a way
// to show it; history and tabs; the search, centered and always there;
// density and, outside macOS, the window's own controls. Its empty space moves the window, and a double
// click maximizes it.
Item {
    id: bar

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
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.topMargin: Metrics.tabBarTop
        // The tabs stop short of the search; what does not fit is cut.
        width: Math.min(implicitWidth, search.x - Metrics.gap12)
        clip: true
        spacing: Metrics.gap4

        IconButton {
            visible: SettingsModel.sidebarMode === "closed"
            tip: I18n.tr("Show the sidebar", I18n.rev)
            glyph: Icons.layoutSidebarLeftExpand
            onClicked: SettingsModel.sidebarMode = "open"
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

        Repeater {
            model: Navigation.tabTitles

            TabButton {
                required property int index
                required property string modelData
                title: modelData
                glyph: Icons[Navigation.tabIcons[index]] || ""
                active: index === Navigation.active
                onActivated: Navigation.activate(index)
                onClosed: Navigation.close(index)
            }
        }

        IconButton {
            tip: I18n.tr("New tab", I18n.rev)
            glyph: Icons.plus
            onClicked: Navigation.open("library", 0)
        }
    }

    // Centered over the content, whatever screen is open; the results open
    // under it.
    SearchBox {
        id: search
        x: (bar.width - width) / 2 // derived: centered over the content
        y: Metrics.tabBarTop
        width: Math.min(Metrics.topSearch, bar.width / 3) // derived: a third of the bar at most
        flyoutX: (width - Metrics.searchFlyout) / 2 // derived: centered under the field
        flyoutY: height + Metrics.gap6
        room: bar.Window.window ? bar.Window.window.height - Metrics.tabBarHeight - Metrics.spaceWindow : 0 // derived: under the tab bar
    }

    Row {
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
            visible: Qt.platform.os !== "osx"
            anchors.verticalCenter: parent.verticalCenter
            width: 1
            height: Metrics.dividerHeight
            color: Theme.color.strokeHairline
        }

        Repeater {
            model: Qt.platform.os === "osx" ? [] : [
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
