import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: panel
    readonly property var state: JSON.parse(Updates.snapshot || "{}")
    visible: state.supported === true
    spacing: Metrics.gap8
    Text { text: I18n.tr("Updates", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.titleSmall }
    Text {
        objectName: "updateStatus"
        width: parent.width
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
        text: panel.state.offline === true && !panel.state.busy ? I18n.tr("Offline", I18n.rev) : panel.state.phase === "checking" ? I18n.tr("Checking for updates…", I18n.rev)
            : panel.state.phase === "available" ? I18n.tr("A new version of Chilpa is available.", I18n.rev)
            : panel.state.phase === "downloading" ? I18n.tr("Downloading update…", I18n.rev)
            : panel.state.phase === "installing" ? I18n.tr("Preparing the updated AppImage…", I18n.rev)
            : panel.state.phase === "cancelling" ? I18n.tr("Cancelling update…", I18n.rev)
            : panel.state.phase === "ready" ? I18n.tr("The update is ready. Restart Chilpa to use it.", I18n.rev)
            : panel.state.phase === "current" ? I18n.tr("Chilpa is up to date.", I18n.rev)
            : panel.state.phase === "error" && panel.state.reason === "read_only" ? I18n.tr("Move this AppImage to a writable folder, then try again.", I18n.rev)
            : panel.state.phase === "error" ? panel.state.can_install === true ? I18n.tr("The update could not be completed. Your current AppImage is safe. Try again later.", I18n.rev) : I18n.tr("The update check failed. Try again.", I18n.rev)
            : panel.state.phase === "cancelled" ? I18n.tr("Update cancelled.", I18n.rev)
            : I18n.tr("Check for a newer version of Chilpa.", I18n.rev)
    }
    ProgressBar {
        objectName: "updateProgress"
        width: parent.width
        visible: panel.state.busy === true
        indeterminate: panel.state.phase !== "downloading" || !panel.state.progress
        value: panel.state.progress || 0
    }
    Row {
        spacing: Metrics.gap8
        Toggle { objectName: "automaticUpdates"; checked: panel.state.automatic === true; onToggled: on => Updates.setAutomatic(on) }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Check for updates automatically", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
    }
    Row {
        spacing: Metrics.gap8
        PillButton { objectName: "checkUpdates"; text: I18n.tr("Check for updates", I18n.rev); enabled: !panel.state.offline && !panel.state.busy && panel.state.phase !== "ready"; onClicked: Updates.check(false) }
        PillButton { objectName: "downloadUpdate"; enabled: !panel.state.offline; text: I18n.tr("Update", I18n.rev); kind: "primary"; visible: panel.state.phase === "available" && panel.state.can_install === true; onClicked: Updates.download() }
        PillButton { objectName: "openUpdateRelease"; enabled: !panel.state.offline && !!panel.state.release_url; text: I18n.tr("Open release page", I18n.rev); visible: panel.state.phase === "available" && panel.state.can_install !== true; onClicked: Qt.openUrlExternally(panel.state.release_url) }
        PillButton { objectName: "cancelUpdate"; text: I18n.tr("Cancel", I18n.rev); visible: panel.state.busy === true && panel.state.phase !== "installing"; enabled: panel.state.phase !== "cancelling"; onClicked: Updates.cancel() }
        PillButton { objectName: "restartUpdate"; text: I18n.tr("Restart Chilpa", I18n.rev); kind: "primary"; visible: panel.state.phase === "ready"; onClicked: if (Updates.restart()) Qt.quit() }
    }
    Text { width: parent.width; visible: panel.state.can_install === true; wrapMode: Text.WordWrap; text: I18n.tr("Updates keep your library, settings and plugins. One previous AppImage is kept beside the new one.", I18n.rev); font.pointSize: Type.small; color: Theme.color.textTertiary }
    Timer { objectName: "updatePolling"; interval: Motion.poll; repeat: true; running: panel.visible && panel.Window.window && panel.Window.window.visible && panel.state.busy === true; onTriggered: Updates.refresh() }
    Component.onCompleted: Updates.refresh()
}
