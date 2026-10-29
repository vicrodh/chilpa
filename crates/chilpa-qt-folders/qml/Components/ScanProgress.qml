import QtQuick
import app.chilpa
import app.chilpa.folders
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I show the actual scan state in Settings, including interruption and failure.
Rectangle {
    id: banner
    objectName: "libraryScanBanner"
    readonly property string phase: LibraryFolders.phase
    readonly property bool failed: phase === "failed" || LibraryFolders.status !== ""
    visible: LibraryFolders.scanning || LibraryFolders.canResume || phase !== "idle" || failed
    implicitHeight: visible ? body.implicitHeight + Metrics.gap12 * 2 : 0 // derived: padding above and below
    radius: Metrics.radiusControl
    color: Theme.color.fillField
    function title() {
        if (failed) return I18n.tr("Library check failed", I18n.rev)
        switch (phase) {
        case "discovering": return (LibraryFolders.found > 0
            ? I18n.trArgs("Looking for files… {} found", [String(LibraryFolders.found)], I18n.rev)
            : I18n.tr("Looking for files…", I18n.rev))
        case "reading": return I18n.trArgs("Reading {} of {} files", [String(LibraryFolders.done), String(LibraryFolders.total)], I18n.rev)
        case "finalizing": return I18n.tr("Updating the library…", I18n.rev)
        case "stopping": return I18n.tr("Stopping after the current file…", I18n.rev)
        case "complete": return I18n.tr("Library check complete", I18n.rev)
        case "interrupted": return I18n.tr("Library check interrupted", I18n.rev)
        default: return I18n.tr("Checking library folders…", I18n.rev)
        }
    }
    Column {
        id: body
        x: Metrics.gap12
        y: Metrics.gap12
        width: parent.width - Metrics.gap12 * 2 // derived: padding on both sides
        spacing: Metrics.gap6
        Item {
            width: parent.width
            height: Math.max(head.implicitHeight, actions.implicitHeight)
            Text {
                id: head
                objectName: "scanTitle"
                width: parent.width - actions.width - Metrics.gap12
                anchors.verticalCenter: parent.verticalCenter
                text: banner.title()
                font.pointSize: Type.body
                font.weight: Type.medium
                color: banner.failed ? Theme.color.statusNegative : Theme.color.textPrimary
                wrapMode: Text.WordWrap
            }
            Row {
                id: actions
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap8
                PillButton {
                    objectName: "stopLibraryScan"
                    visible: LibraryFolders.scanning
                    enabled: banner.phase !== "stopping"
                    text: I18n.tr("Stop scan", I18n.rev)
                    onClicked: LibraryFolders.stopScan()
                }
                PillButton {
                    objectName: "resumeLibraryScan"
                    visible: !LibraryFolders.scanning && LibraryFolders.canResume
                    text: I18n.tr("Continue", I18n.rev)
                    glyph: Lucide.circlePlay
                    iconFamily: Lucide.font
                    onClicked: LibraryFolders.resumeScan()
                }
                PillButton {
                    objectName: "dismissPendingLibraryScan"
                    visible: !LibraryFolders.scanning && LibraryFolders.canResume
                    kind: "ghost"
                    text: I18n.tr("Dismiss scan", I18n.rev)
                    onClicked: LibraryFolders.dismissStatus()
                }
                PillButton {
                    objectName: "retryLibraryScan"
                    visible: banner.failed && !LibraryFolders.scanning && !LibraryFolders.canResume
                    enabled: LibraryFolders.count > 0
                    text: I18n.tr("Retry", I18n.rev)
                    onClicked: LibraryFolders.rescanAll()
                }
                IconButton {
                    objectName: "dismissLibraryScan"
                    visible: !LibraryFolders.scanning && !LibraryFolders.canResume
                    tip: I18n.tr("Dismiss", I18n.rev)
                    glyph: Lucide.x
                    iconFamily: Lucide.font
                    onClicked: LibraryFolders.dismissStatus()
                }
            }
        }
        Text {
            objectName: "scanRoot"
            visible: text !== ""
            width: parent.width
            text: LibraryFolders.scanningPath
            elide: Text.ElideMiddle
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Rectangle {
            visible: LibraryFolders.scanning
            width: parent.width
            height: Metrics.progress
            radius: Metrics.hairline
            color: Theme.color.strokeHairline
            Rectangle {
                objectName: "scanProgressFill"
                width: parent.width * Math.max(0, Math.min(1, LibraryFolders.progress))
                height: parent.height
                radius: Metrics.hairline
                color: Theme.color.accent
            }
        }
        Text {
            objectName: "scanSummary"
            visible: !LibraryFolders.scanning
            width: parent.width
            text: I18n.trArgs("{} read, {} unchanged, {} missing", [String(LibraryFolders.readCount), String(LibraryFolders.sameCount), String(LibraryFolders.missingCount)], I18n.rev)
                + " · " + I18n.trArgs("{} unreadable", [String(LibraryFolders.unreadableCount)], I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Text {
            objectName: "scanError"
            visible: text !== ""
            width: parent.width
            text: LibraryFolders.status
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.statusNegative
        }
        Text {
            objectName: "scanUnavailable"
            visible: LibraryFolders.unreachable
            width: parent.width
            text: I18n.tr("Folders are unavailable. Their existing tracks remain in the library.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.statusNegative
        }
        Text {
            objectName: "scanResumeHint"
            visible: !LibraryFolders.scanning && LibraryFolders.canResume
            width: parent.width
            text: I18n.tr("Completed work is saved. Continue checks the folder again and skips unchanged files.", I18n.rev)
                + " " + I18n.tr("Automatic checks are paused until you continue or dismiss this scan. Dismissing keeps the files already added.", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
    }
}
