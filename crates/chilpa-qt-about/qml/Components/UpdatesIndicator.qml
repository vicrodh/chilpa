import QtQuick
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Item {
    id: inbox
    property var review: function() {}
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
    Component.onCompleted: PluginUpdates.refresh()
    // The Welcome and What's new; a card's button goes where `review` leads.
    OnboardingModal { onActionChosen: action => inbox.review(action) }
    Modal {
        id: appDetails
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
