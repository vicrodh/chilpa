import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
// The tabs, in the room the bar leaves them and never over what is beside
// them. When they do not fit they scroll: by dragging, by the wheel, or by
// the two arrows that appear at their ends, which are not the arrows that
// go back and forward. The active tab is brought into sight.
Row {
    id: strip

    // The width the bar leaves for the tabs, their arrows and the button
    // that opens a new one.
    property real room: 0

    readonly property bool crowded: tabs.width + add.width + spacing > room
    readonly property real window: crowded
        ? Math.max(0, room - add.width - (before.width + spacing) * 2 - spacing) // derived: what the arrows and the button leave
        : tabs.width

    function reveal(index) {
        const tab = repeater.itemAt(index)
        if (!tab || !crowded)
            return
        if (tab.x < view.contentX)
            slideTo(tab.x)
        else if (tab.x + tab.width > view.contentX + view.width)
            slideTo(tab.x + tab.width - view.width)
    }
    function step(by) {
        slideTo(view.contentX + by * Metrics.tabMaxWidth)
    }
    // Only what the arrows, the wheel and a change of tab ask for is
    // animated: a drag moves the tabs itself, and an animation on every
    // change of position fought it.
    function slideTo(x) {
        slide.stop()
        slide.to = Math.max(0, Math.min(view.contentWidth - view.width, x))
        slide.start()
    }

    NumberAnimation {
        id: slide
        target: view
        property: "contentX"
        duration: Motion.panel
        easing.type: Easing.OutCubic
    }

    spacing: Metrics.gap4
    IconButton {
        id: before
        visible: strip.crowded
        size: Metrics.button24
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.chevronCompactLeft
        tip: I18n.tr("Earlier tabs", I18n.rev)
        enabled: view.contentX > 0
        onClicked: strip.step(-1)
    }

    Flickable {
        id: view
        width: strip.window
        height: Metrics.tabHeight
        contentWidth: tabs.width
        contentHeight: height
        clip: true
        interactive: strip.crowded
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.horizontal: ViewportScrollBar { gutter: Metrics.gap4 }
        onDragStarted: slide.stop()

        Row {
            id: tabs
            spacing: strip.spacing

            Repeater {
                id: repeater
                model: Navigation.tabTitles

                TabButton {
                    required property int index
                    required property string modelData
                    title: modelData
                    glyph: Icons[Navigation.tabIcons[index]] || ""
                    sourceIcon: !Icons[Navigation.tabIcons[index]] ? Navigation.tabIcons[index] || "" : ""
                    active: index === Navigation.active
                    onActivated: Navigation.activate(index)
                    onClosed: Navigation.close(index)
                }
            }
        }

        // no cursor: the wheel over the tabs moves them
        WheelHandler {
            enabled: strip.crowded
            onWheel: (event) => strip.step(-(event.angleDelta.y || event.angleDelta.x) / Metrics.wheelNotch / 2) // derived: half a tab a notch
        }
    }

    IconButton {
        visible: strip.crowded
        size: Metrics.button24
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.chevronCompactRight
        tip: I18n.tr("Later tabs", I18n.rev)
        enabled: view.contentX < view.contentWidth - view.width
        onClicked: strip.step(1)
    }

    IconButton {
        id: add
        tip: I18n.tr("New tab", I18n.rev)
        glyph: Icons.plus
        onClicked: Navigation.open("library", 0)
    }

    Connections {
        target: Navigation
        function onActiveChanged() { Qt.callLater(strip.reveal, Navigation.active) }
    }
    onCrowdedChanged: Qt.callLater(strip.reveal, Navigation.active)
}
