import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: footer
    property var status: ({})
    property var document: ({})
    readonly property string token: status.phase === "review" ? status.plan || "" : ""
    signal previewAsked()
    signal recoverAsked()
    signal syncAsked(string token, bool removals)
    signal cancelAsked()
    spacing: Metrics.gap8
    onTokenChanged: approval.checked = false

    Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    Text {
        width: parent.width
        text: footer.status.label || ""
        font.pointSize: Type.body
        font.weight: Type.semibold
        wrapMode: Text.WordWrap
        color: Theme.color.textPrimary
    }
    Text {
        width: parent.width
        visible: footer.status.phase === "review" || footer.status.phase === "done"
        text: (footer.status.summary || "") + (footer.status.required ? " · " + I18n.trArgs("{} required", [footer.status.required], I18n.rev) : "")
        font.pointSize: Type.small
        wrapMode: Text.WordWrap
        color: Theme.color.textSecondary
    }
    Column {
        width: parent.width
        visible: footer.status.busy === true || (["error", "cancelled"].includes(footer.status.phase) && (footer.status.total || 0) > 0)
        spacing: Metrics.gap4
        Rectangle {
            width: parent.width
            height: Metrics.progress
            radius: height
            color: Theme.color.strokeHairline
            Rectangle { width: parent.width * (footer.status.progress || 0); height: parent.height; radius: height; color: Theme.color.accent }
        }
        Text { width: parent.width; visible: (footer.status.total || 0) > 0; text: footer.status.progress_text || ""; font.pointSize: Type.small; color: Theme.color.textSecondary; elide: Text.ElideRight }
        Text { width: parent.width; visible: text !== ""; text: footer.status.current || ""; textFormat: Text.PlainText; font.pointSize: Type.small; color: Theme.color.textSecondary; elide: Text.ElideRight }
    }
    CircleCheck {
        id: approval
        objectName: "deviceApproveRemovals"
        width: parent.width
        visible: footer.token !== "" && (footer.status.removals || 0) > 0
        enabled: !footer.status.busy
        text: I18n.trCount("I approve removing {} managed file.", "I approve removing {} managed files.", footer.status.removals || 0, I18n.rev)
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap8
        PillButton {
            objectName: "devicePreview"
            visible: !footer.status.busy && footer.status.error?.recovery !== true
            text: I18n.tr("Review changes", I18n.rev)
            enabled: footer.document.can_review === true
            onClicked: footer.previewAsked()
        }
        PillButton {
            objectName: "deviceRecover"
            visible: !footer.status.busy && footer.status.error?.recovery === true
            text: I18n.tr("Recover interrupted sync", I18n.rev)
            kind: "primary"
            onClicked: footer.recoverAsked()
        }
        PillButton {
            objectName: "deviceSyncNow"
            visible: !footer.status.busy && footer.token !== ""
            kind: "primary"
            text: I18n.tr("Sync now", I18n.rev)
            enabled: (footer.document.change_count || 0) > 0 && ((footer.status.removals || 0) === 0 || approval.checked)
            onClicked: footer.syncAsked(footer.token, approval.checked)
        }
        PillButton { objectName: "deviceCancel"; visible: footer.status.busy === true; text: I18n.tr("Cancel", I18n.rev); enabled: footer.status.phase !== "cancelling"; onClicked: footer.cancelAsked() }
        Text { height: Metrics.pillHeight; verticalAlignment: Text.AlignVCenter; visible: !footer.status.busy && footer.status.error?.recovery !== true; text: I18n.tr("Original files, with playlists", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
    }
}
