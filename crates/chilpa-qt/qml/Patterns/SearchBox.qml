import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The search field and what it finds (design/03 SearchFlyout, design/02 8):
// typing opens the results where the owner puts them -- beside the sidebar,
// or under the tab bar's field when the sidebar is closed. Compact, there is
// no field to type in and the results bring their own (a collapsed sidebar).
// Ctrl+K reaches it while it is the active one.
Item {
    id: box

    property bool active: true
    property bool compact: false
    property real flyoutX: 0
    property real flyoutY: 0
    property int room: 0

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

    Shortcut {
        enabled: box.active
        sequence: "Ctrl+K"
        onActivated: box.focusSearch()
    }

    SearchField {
        id: field
        anchors.fill: parent
        visible: !box.compact
        placeholder: I18n.tr("Search", I18n.rev)
        shortcut: Qt.platform.os === "osx" ? "⌘K" : "Ctrl K"
        onTextChanged: {
            search.text = text
            text !== "" ? flyout.open() : flyout.close()
        }
        onEditingChanged: if (editing && text !== "") flyout.open()
        onStepped: (by) => flyout.step(by)
        onAccepted: flyout.chooseCurrent()
        onEscaped: {
            flyout.close()
            release()
        }
    }

    SearchFlyout {
        id: flyout
        search: search
        x: box.flyoutX
        y: box.flyoutY
        room: box.room
        onClosed: if (!box.compact) field.release()
    }
}
