import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Tools
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The search field and what it finds (design/03 SearchFlyout, design/02 8):
// typing opens the results where the owner puts them -- beside the sidebar,
// or under the tab bar's field when the sidebar is closed. Compact, there is
// no field to type in and the results bring their own (a collapsed sidebar).
// Its shortcut reaches it while it is the active one.
Item {
    id: box

    property bool active: true
    property bool compact: false
    property real flyoutX: 0
    property real flyoutY: 0
    property int room: 0
    // Results under the box as you type; off, typing opens the results page.
    readonly property bool preview: Prefs.rev >= 0 && SettingsModel.kept("navigation.searchPreview") !== "false"
    function openPage(text) {
        if (text.trim().length >= 2) Navigation.goKey("search", text.trim())
    }

    function focusSearch() {
        if (compact) {
            flyout.ownInput = true
            flyout.open()
        } else {
            field.focusInput()
        }
    }

    implicitHeight: Metrics.fieldHeight

    SearchModel { id: search }
    Tools.SearchCommands { active: box.active; host: box; search: search; field: field; flyout: flyout }

    Connections {
        target: Hotkeys
        function onSearchAsked() { if (box.active) box.focusSearch() }
    }

    Connections {
        target: Navigation
        function onSearchOriginRequested(query, source) {
            if (!box.active) return
            if (!box.preview) { box.openPage(query); return }
            search.source = source || "all"
            search.text = query
            field.text = query
            flyout.ownInput = box.compact
            flyout.open()
        }
    }

    SearchField {
        id: field
        anchors.fill: parent
        visible: !box.compact
        placeholder: I18n.tr("Search", I18n.rev)
        shortcut: Qt.platform.os === "osx" ? "⌘K" : "Ctrl K"
        onTextChanged: {
            search.text = text
            if (box.preview) text !== "" ? flyout.open() : flyout.close()
            else text.trim().length >= 2 ? live.restart() : live.stop()
        }
        onEditingChanged: if (editing && text !== "" && box.preview) flyout.open()
        onStepped: (by) => flyout.step(by)
        // Without the preview, Enter opens the page and clears the box, as QBZ.
        onAccepted: if (box.preview) flyout.chooseCurrent(); else { live.stop(); box.openPage(text); text = "" }
        onEscaped: {
            flyout.close()
            release()
        }
    }

    // A pause in typing opens the page, without the preview.
    Timer { id: live; interval: Motion.searchLive; onTriggered: box.openPage(field.text) }

    SearchFlyout {
        id: flyout
        onAllAsked: { close(); box.openPage(search.text) }
        search: search
        x: box.flyoutX
        y: box.flyoutY
        room: box.room
        onClosed: if (!box.compact) field.release()
    }
}
