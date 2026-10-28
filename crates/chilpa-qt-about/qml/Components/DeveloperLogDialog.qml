import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The log viewer, laid out as QBZ's (`LogViewerModal.qml`). The default view
// is the header (title, shown / total, the disclosure and a close), the column
// header, the list, the address of an upload once there is one, and exactly
// two buttons: Copy all and Upload. The disclosure reveals the level, the
// search, Refresh and Auto-tail, and a row for the bundle, the log file and
// Clear. It opens closed every time.
Modal {
    id: dialog
    property bool advanced: false
    property bool autoTail: false
    // Copy all says Copied for a beat after any copy, as QBZ's does.
    property bool copied: false
    readonly property var document: JSON.parse(Developers.logs || "{}")
    width: Math.min(parent.width - Metrics.modalInsets * 2, 760) // derived: the window less a margin, at most QBZ's 760
    height: Math.min(parent.height - Metrics.modalInsets * 2, 640) // derived: the window less a margin, at most QBZ's 640
    function refresh() { Developers.refreshLogs(controls.level, controls.search) }
    function flashCopied() { dialog.copied = true; copiedBeat.restart() }
    onOpened: { advanced = false; autoTail = false; controls.reset(); refresh() }
    onClosed: { autoTail = false; Developers.releaseLogs() }
    // The log follows itself once a second, and only while it is open and asked to.
    Timer { interval: 1000; repeat: true; running: dialog.visible && dialog.autoTail; onTriggered: dialog.refresh() }
    Timer { id: copiedBeat; interval: 1000; onTriggered: dialog.copied = false }

    Item {
        id: headerRow
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: Metrics.button28
        Row {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap10
            Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Logs", I18n.rev); font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
            // Shown over total tells "the filter matched nothing" from "the ring is empty".
            Text { anchors.verticalCenter: parent.verticalCenter; text: (dialog.document.shown || 0) + " / " + (dialog.document.total || 0); font.pointSize: Type.small; color: Theme.color.textTertiary }
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap10
            Item {
                objectName: "developerLogOptions"
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.button24
                height: Metrics.button24
                Icon {
                    anchors.centerIn: parent
                    glyph: dialog.advanced ? Icons.minus : Icons.plus
                    size: Metrics.iconNav
                    color: optionsArea.containsMouse ? Theme.color.textPrimary : Theme.color.textTertiary
                }
                MouseArea { id: optionsArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: dialog.advanced = !dialog.advanced }
            }
            Item {
                anchors.verticalCenter: parent.verticalCenter
                width: Metrics.button28
                height: Metrics.button28
                Icon {
                    anchors.centerIn: parent
                    glyph: Icons.x
                    size: Metrics.iconNav
                    color: closeArea.containsMouse ? Theme.color.textPrimary : Theme.color.textTertiary
                }
                MouseArea { id: closeArea; objectName: "developerLogClose"; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: dialog.close() }
            }
        }
    }

    LogControls {
        id: controls
        anchors.top: headerRow.bottom
        anchors.topMargin: Metrics.gap14
        anchors.left: parent.left
        anchors.right: parent.right
        visible: dialog.advanced
        autoTail: dialog.autoTail
        onChanged: dialog.refresh()
        onRefreshAsked: dialog.refresh()
        onAutoTailAsked: on => dialog.autoTail = on
    }

    LogList {
        anchors.top: dialog.advanced ? controls.bottom : headerRow.bottom
        anchors.topMargin: Metrics.gap14
        anchors.left: parent.left
        anchors.right: parent.right
        // Stops at whichever optional row is mounted below it.
        anchors.bottom: uploaded.visible ? uploaded.top : (dialog.advanced ? actions.top : footer.top)
        anchors.bottomMargin: Metrics.gap14
        rows: dialog.visible ? dialog.document.rows || [] : []
        autoTail: dialog.autoTail
    }

    LogUploaded {
        id: uploaded
        objectName: "developerUploadedUrl"
        anchors.bottom: dialog.advanced ? actions.top : footer.top
        anchors.bottomMargin: Metrics.gap14
        anchors.left: parent.left
        anchors.right: parent.right
        url: Developers.uploadedUrl
        error: Developers.uploadError
        onCopyAsked: { Developers.copyUrl(); dialog.flashCopied() }
    }

    Item {
        id: actions
        anchors.bottom: footer.top
        anchors.bottomMargin: Metrics.gap14
        anchors.left: parent.left
        anchors.right: parent.right
        height: Metrics.button32
        visible: dialog.advanced
        LogActionButton {
            objectName: "developerCopyBundle"
            anchors.left: parent.left
            text: I18n.tr("Copy diagnostics bundle", I18n.rev)
            glyph: Icons.copy
            onClicked: { Developers.copyBundle(); dialog.flashCopied() }
        }
        Row {
            anchors.right: parent.right
            spacing: Metrics.gap10
            LogActionButton {
                objectName: "developerOpenLogFile"
                text: I18n.tr("Open log file", I18n.rev)
                glyph: Icons.externalLink
                enabled: dialog.document.fileAvailable === true
                onClicked: Qt.openUrlExternally(Developers.logUrl())
            }
            LogActionButton {
                objectName: "developerClearLogs"
                text: I18n.tr("Clear", I18n.rev)
                glyph: Icons.trash
                danger: true
                onClicked: { Developers.clearLogs(); dialog.refresh() }
            }
        }
    }

    // Always visible: the two things this window is for.
    Row {
        id: footer
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        spacing: Metrics.gap10
        LogActionButton {
            objectName: "developerCopyAll"
            text: dialog.copied ? I18n.tr("Copied", I18n.rev) : I18n.tr("Copy all", I18n.rev)
            glyph: dialog.copied ? Icons.check : Icons.copy
            onClicked: { Developers.copyLogs(controls.level, controls.search); dialog.flashCopied() }
        }
        LogActionButton {
            objectName: "developerUploadBundle"
            text: Developers.uploading ? I18n.tr("Uploading…", I18n.rev) : I18n.tr("Upload (public)", I18n.rev)
            glyph: Lucide.cloudUpload
            iconFamily: Lucide.font
            enabled: !Developers.uploading
            onClicked: Developers.uploadBundle()
        }
    }
}
