import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What the startup cover says when the library could not be upgraded (the
// facade's `library_upgrade_view`): what happened, that nothing was lost
// and where the copy from before is, and what can be done from here. The
// window behind it never opens on that library.
Column {
    id: failure

    property var upgrade: ({})
    signal retry()
    signal quit()
    signal restore()

    // The keyboard: Tab and Shift+Tab go round these buttons only, Enter or
    // Space press the one in focus, and Escape quits.
    function buttons() { return [retryButton, restoreButton, quitButton].filter(button => button.visible) }
    function focusFirst() {
        const shown = buttons()
        if (shown.length > 0) shown[0].forceActiveFocus()
    }
    // Shown, it takes the keyboard. Once the panel is visible its buttons
    // are too, and which of them shows settles with the rest of the bindings.
    onVisibleChanged: if (visible) Qt.callLater(focusFirst)
    function step(by) {
        const shown = buttons()
        if (shown.length === 0) return
        const at = shown.findIndex(button => button.activeFocus)
        shown[(Math.max(at, 0) + by + shown.length) % shown.length].forceActiveFocus()
    }
    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Tab) step(1)
        else if (event.key === Qt.Key_Backtab) step(-1)
        else if (event.key === Qt.Key_Escape) failure.quit()
        else return
        event.accepted = true
    }

    spacing: Metrics.gap12

    Text {
        objectName: "upgradeFailureTitle"
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: failure.upgrade.title || ""
        wrapMode: Text.Wrap
        font.pointSize: Type.titleSmall
        font.weight: Type.bold
        color: Theme.color.textPrimary
    }
    Text {
        objectName: "upgradeFailureMessage"
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        visible: text !== ""
        text: failure.upgrade.message || ""
        wrapMode: Text.Wrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    Text {
        objectName: "upgradeFailureDetail"
        width: parent.width
        horizontalAlignment: Text.AlignHCenter
        text: failure.upgrade.detail || ""
        wrapMode: Text.Wrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: Metrics.gap8
        PillButton { id: quitButton; objectName: "upgradeQuit"; text: I18n.tr("Quit", I18n.rev); onClicked: failure.quit() }
        // Not while the library is missing: running the update would make an empty one.
        PillButton { id: retryButton; objectName: "upgradeRetry"; kind: "primary"; visible: failure.upgrade.canRetry !== false; text: I18n.tr("Try again", I18n.rev); onClicked: failure.retry() }
    }
    // Only once a step has committed: before that the library already is
    // the copy.
    PillButton {
        id: restoreButton
        objectName: "upgradeRestore"
        anchors.horizontalCenter: parent.horizontalCenter
        visible: failure.upgrade.canRestore === true
        kind: "ghost"
        wrapText: true
        maximumWidth: parent.width
        text: I18n.tr("Put the copy back and quit", I18n.rev)
        onClicked: failure.restore()
    }
}
