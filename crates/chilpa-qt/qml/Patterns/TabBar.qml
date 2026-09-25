import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The strip above the content (design/02 3): history, tabs, density and,
// outside macOS, the window's own controls. Its empty space moves the
// window, and a double click maximizes it.
Item {
    id: bar

    implicitHeight: Metrics.tabBarHeight

    DragHandler {
        target: null
        onActiveChanged: if (active) bar.Window.window.startSystemMove()
    }
    TapHandler {
        onDoubleTapped: bar.Window.window.visibility === Window.Maximized ? bar.Window.window.showNormal() : bar.Window.window.showMaximized()
    }

    Row {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.topMargin: Metrics.tabBarTop
        spacing: Metrics.gap4

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
