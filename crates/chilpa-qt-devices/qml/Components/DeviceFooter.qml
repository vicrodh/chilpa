import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: footer
    property var status: ({})
    property var document: ({})
    readonly property string token: status.phase === "review" ? status.plan || "" : ""
    property bool progressExpanded: false
    readonly property bool reviewing: ["preparing", "downloading", "cancelling"].includes(status.phase) && !!status.review_stage
    onReviewingChanged: if (reviewing) progressExpanded = true
    signal previewAsked()
    signal recoverAsked()
    signal syncAsked(string token, bool removals, bool verifyCopies)
    signal cancelAsked()
    spacing: Metrics.gap8
    onTokenChanged: { approval.checked = false; if (token !== "") verification.checked = false }

    Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    Row {
        width: parent.width
        spacing: Metrics.gap8
        IconButton {
            objectName: "deviceReviewDetails"
            visible: footer.reviewing
            glyph: footer.progressExpanded ? Icons.chevronDown : Icons.chevronRight
            tip: footer.progressExpanded ? I18n.tr("Hide review details", I18n.rev) : I18n.tr("Show review details", I18n.rev)
            onClicked: footer.progressExpanded = !footer.progressExpanded
        }
        Text {
            width: Math.max(0, parent.width - (footer.reviewing ? Metrics.button28 + Metrics.gap8 : 0))
            height: Math.max(implicitHeight, footer.reviewing ? Metrics.button28 : 0)
            verticalAlignment: Text.AlignVCenter
            text: footer.reviewing && footer.status.phase !== "cancelling" ? footer.status.review_stage_label || footer.status.label || "" : footer.status.label || ""
            font.pointSize: Type.body
            font.weight: Type.semibold
            wrapMode: Text.WordWrap
            color: Theme.color.textPrimary
        }
    }
    Column {
        objectName: "deviceReviewProgress"
        width: parent.width
        visible: footer.reviewing && footer.progressExpanded
        spacing: Metrics.gap4
        Text { width: parent.width; text: [footer.status.review_progress_text, footer.status.selected_bytes].filter(Boolean).join(" · "); color: Theme.color.textSecondary; font.pointSize: Type.small; elide: Text.ElideRight }
        Text { width: parent.width; text: footer.status.current || ""; textFormat: Text.PlainText; color: Theme.color.textSecondary; font.pointSize: Type.small; elide: Text.ElideMiddle }
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
        visible: !footer.reviewing && (footer.status.busy === true || (["error", "cancelled"].includes(footer.status.phase) && (footer.status.total || 0) > 0))
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
            onClicked: footer.syncAsked(footer.token, approval.checked, verification.checked)
        }
        PillButton { objectName: "deviceCancel"; visible: footer.status.busy === true; text: I18n.tr("Cancel", I18n.rev); enabled: footer.status.phase !== "cancelling"; onClicked: footer.cancelAsked() }
        Text { width: Math.min(implicitWidth, parent.width); height: Math.max(Metrics.pillHeight, implicitHeight); wrapMode: Text.WordWrap; verticalAlignment: Text.AlignVCenter; visible: !footer.status.busy && footer.status.error?.recovery !== true; text: footer.status.review_note || I18n.tr("Original music files", I18n.rev); font.pointSize: Type.small; color: Theme.color.textSecondary }
    }
    CircleCheck {
        id: verification
        objectName: "deviceVerifyCopies"
        width: parent.width
        visible: footer.token !== "" || footer.status.phase === "transferring"
        enabled: !footer.status.busy && ((footer.status.copies || 0) + (footer.status.updates || 0)) > 0
        text: I18n.tr("Verify files after transfer", I18n.rev)
    }
}
