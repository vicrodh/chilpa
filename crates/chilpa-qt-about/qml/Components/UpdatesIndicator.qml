import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Item {
    id: inbox
    property var review: function() {}
    // The menu's button the dot sits on; the update bubble points at it.
    property Item anchorItem: parent
    // A restart's helper waits for Chilpa to end: the window quits for real.
    signal quitAsked()
    property string dismissed: ""
    property bool launchChecked: false
    readonly property var appState: JSON.parse(Updates.snapshot || "{}")
    readonly property var pluginState: JSON.parse(PluginUpdates.snapshot || "{}")
    readonly property string appIdentity: (appState.version || "") + ":" + (appState.phase || "")
    readonly property bool appUnread: ["available", "ready"].includes(appState.phase) && pluginState.seen_app !== appIdentity
    readonly property bool unread: appUnread || (pluginState.unread || 0) > 0
    width: Frame.notificationDot; height: Frame.notificationDot
    function openApp() { PluginUpdates.command("updates_app_read", JSON.stringify({identity:appIdentity})); appDetails.open() }
    function openPlugins() { PluginUpdates.command("updates_read", "{}"); pluginDetails.open() }
    // The menu's Chilpa and Plugins updates and its What's new.
    function open(what) { what === "appUpdates" ? openApp() : what === "pluginUpdates" ? openPlugins() : Onboarding.command("whatsNew") }
    Rectangle { objectName: "updatesNotificationDot"; anchors.fill: parent; radius: width / 2; color: Theme.color.statusNegative; visible: inbox.unread } // derived: a circular dot
    Timer { interval: Motion.accountCheck; repeat: true; running: inbox.Window.window && inbox.Window.window.visible; onTriggered: PluginUpdates.refresh() }
    Timer { interval: Frame.updateDelay; running: !inbox.launchChecked; onTriggered: { inbox.launchChecked = true; Updates.check(true) } }
    Timer { interval: Motion.poll; repeat: true; running: inbox.appState.busy === true && inbox.Window.window && inbox.Window.window.visible; onTriggered: Updates.refresh() }
    Connections { target: Updates; function onRestarting() { inbox.quitAsked() } }
    Component.onCompleted: { PluginUpdates.refresh(); Updates.refresh() }
    // Out of the app menu while a version is on offer and unread, until it
    // is opened or dismissed.
    Controls.Popup {
        id: bubble
        objectName: "updateBubble"
        PopupInput { surface: bubble }
        readonly property Item button: inbox.anchorItem || inbox
        parent: inbox
        visible: inbox.appUnread && inbox.dismissed !== inbox.appIdentity && !!inbox.Window.window
        modal: false
        focus: false
        closePolicy: Controls.Popup.NoAutoClose
        padding: 0
        background: null
        transformOrigin: Controls.Popup.TopRight
        x: bubble.button.width - width - inbox.x // derived: right edge on the button's
        y: bubble.button.height + Metrics.gap10 - inbox.y // derived: under the button
        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Motion.panel }
            NumberAnimation { property: "scale"; from: Frame.noticeGrowth; to: 1; duration: Motion.panel; easing.type: Easing.OutBack }
        }
        exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: Motion.fast } }
        contentItem: UpdateNotice {
            state: inbox.appState
            caretX: width - bubble.button.width / 2 // derived: under the button's middle
            onViewAsked: inbox.openApp()
            onDismissed: inbox.dismissed = inbox.appIdentity
        }
    }
    // The Welcome and What's new; a card's button goes where `review` leads.
    OnboardingModal { onActionChosen: action => inbox.review(action) }
    Modal {
        id: appDetails
        objectName: "appUpdateDetails"
        width: Frame.updateWidth + Metrics.modalInsets
        Loader { width: appDetails.width - Metrics.modalInsets; active: appDetails.opened; sourceComponent: UpdatesPanel {} }
    }
    Modal {
        id: pluginDetails
        width: Frame.widest
        Loader {
            width: pluginDetails.width - Metrics.modalInsets
            active: pluginDetails.opened
            sourceComponent: PluginUpdatesPanel { review: function() { pluginDetails.close(); inbox.review() } }
        }
    }
}
