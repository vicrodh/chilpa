import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Theme"

// The window's cover while it reads what has to be read before it takes a
// click: the plugins, which pages, search and playback lean on, and the
// library's first reading, which brings its records, its columns' entries
// and its sources' counts (its switches, columns and Refine are read from
// the facade as they are built). Each has a bar and a line that says what
// it is, and the cover goes when every one is done. A server's sync is not
// one of them: it goes on behind, and the lists read the library again each
// time it writes.
//
// A library older than this Chilpa is upgraded first, and that is the first
// bar: nothing reads the library and the plugins do not start until it is
// done, however long it takes, and if it fails the cover says so and stays.
Rectangle {
    id: splash

    readonly property var view: JSON.parse(Sources.snapshot || "{}")
    readonly property var startup: view.startup || ({})
    readonly property var upgrade: JSON.parse(LibraryUpgrade.snapshot || "{}")
    readonly property bool upgrading: upgrade.active === true
    readonly property bool failed: upgrading && upgrade.failed === true
    // Read once: a later reading of the library does not bring the cover back.
    property bool libraryRead: false
    // A reading that never answers does not keep the window shut: past this
    // the cover goes, and the log says so. An upgrade is waited for whole.
    property int limit: Sizes.startupLimit
    property bool given: false
    readonly property var tasks: (upgrading ? [
        { done: false, progress: upgrade.progress, text: upgrade.text || "" }
    ] : []).concat([
        { done: !upgrading && view.starting === false, progress: startup.total > 0 ? startup.done / startup.total : -1,
          text: startup.name ? I18n.trArgs("Starting {}…", [startup.name], I18n.rev) : I18n.tr("Starting plugins…", I18n.rev) },
        { done: libraryRead, progress: -1, text: I18n.tr("Reading the library…", I18n.rev) }
    ])
    readonly property bool waiting: upgrading || (!given && tasks.some(task => !task.done))

    // The library is read once something of it arrived: its first answer is
    // never given up for a newer request (`Library::refresh`). What arrives
    // while it is upgraded is not the library's answer.
    function seeLibrary() { if (!upgrading && (!Library.busy || Library.rev > 0)) libraryRead = true }
    function look() { LibraryUpgrade.refresh(); Sources.refresh() }
    onUpgradingChanged: seeLibrary()
    Connections {
        target: Library
        function onBusyChanged() { splash.seeLibrary() }
        function onRevChanged() { splash.seeLibrary() }
    }

    color: Theme.color.surfaceWindow
    visible: opacity > 0
    opacity: waiting ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Motion.panel } }

    Component.onCompleted: { look(); seeLibrary() }
    Timer { interval: Motion.poll; repeat: true; running: splash.waiting; onTriggered: splash.look() }
    Timer {
        interval: splash.limit
        running: splash.waiting && !splash.upgrading
        onTriggered: {
            console.warn("startup: still reading after " + interval + " ms; the window opens without waiting")
            splash.given = true
        }
    }

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
        // One bar a task, kept while the tasks change how far they are.
        Repeater {
            model: splash.failed ? 0 : splash.tasks.length
            StartupTask {
                required property int index
                objectName: "startupTask"
                width: parent.width
                text: splash.tasks[index].text
                progress: splash.tasks[index].progress
                done: splash.tasks[index].done
            }
        }
        Text {
            objectName: "upgradeNote"
            width: parent.width
            visible: splash.upgrading && !splash.failed
            horizontalAlignment: Text.AlignHCenter
            text: splash.upgrade.note || ""
            wrapMode: Text.Wrap
            font.pointSize: Type.micro
            color: Theme.color.textSecondary
        }
        UpgradeFailure {
            objectName: "upgradeFailure"
            width: parent.width
            visible: splash.failed
            upgrade: splash.upgrade
            onRetry: LibraryUpgrade.retry()
            onQuit: LibraryUpgrade.quit()
        }
    }
}
