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
        PillButton { objectName: "upgradeQuit"; text: I18n.tr("Quit", I18n.rev); onClicked: failure.quit() }
        PillButton { objectName: "upgradeRetry"; kind: "primary"; text: I18n.tr("Try again", I18n.rev); onClicked: failure.retry() }
    }
    // Only once a step has committed: before that the library already is
    // the copy.
    PillButton {
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
