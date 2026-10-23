import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The window's cover while the plugins start: their pages, search and
// playback lean on them, so nothing under it is reachable until they are up.
// A bar fills as each plugin loads, and the one loading is named.
Rectangle {
    id: splash

    readonly property var view: JSON.parse(Sources.snapshot || "{}")
    readonly property var startup: view.startup || ({})
    // It covers from the first frame, until the facade says the plugins are up.
    readonly property bool waiting: view.starting !== false

    color: Theme.color.surfaceWindow
    visible: opacity > 0
    opacity: waiting ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Motion.panel } }

    Component.onCompleted: Sources.refresh()
    Timer { interval: Motion.poll; repeat: true; running: splash.waiting; onTriggered: Sources.refresh() }

    // Nothing under it takes a click or a key while it covers the window.
    // no cursor: it only swallows input, there is nothing to click
    MouseArea { anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.AllButtons; preventStealing: true; onWheel: (wheel) => wheel.accepted = true }
    Keys.onPressed: (event) => event.accepted = true
    focus: waiting

    Column {
        anchors.centerIn: parent
        width: Metrics.confirmWidth
        spacing: Metrics.gap12

        // The one place the logo shows by itself: the cover the window wears
        // while it starts.
        Image {
            objectName: "splashLogo"
            anchors.horizontalCenter: parent.horizontalCenter
            width: Metrics.artRecord * 2; height: width // derived: twice a record's art
            source: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"
            sourceSize: Qt.size(width * 2, height * 2) // derived: sharp at twice the density
            fillMode: Image.PreserveAspectFit
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "Chilpa"
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        Rectangle {
            width: parent.width
            height: Metrics.progress
            radius: 1
            color: Theme.color.strokeHairline

            Rectangle {
                width: parent.width * (splash.startup.total > 0 ? splash.startup.done / splash.startup.total : 0)
                height: parent.height
                radius: 1
                color: Theme.color.accent
                Behavior on width { NumberAnimation { duration: Motion.fast } }
            }
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: splash.startup.name ? I18n.trArgs("Starting {}…", [splash.startup.name], I18n.rev)
                : I18n.tr("Starting plugins…", I18n.rev)
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }
}
