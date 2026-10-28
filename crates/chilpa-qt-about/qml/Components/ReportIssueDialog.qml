import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: dialog
    objectName: "reportIssueDialog"
    property bool listenForRequests: true
    property var openExternal: function(url) { Qt.openUrlExternally(url) }
    width: Math.min(Metrics.confirmWidth + Metrics.modalInsets, parent.width - Metrics.modalInsets)
    height: Math.min(body.implicitHeight + Metrics.modalInsets, parent.height - Metrics.modalInsets)
    onClosed: review.close()
    Connections {
        target: About
        enabled: dialog.listenForRequests
        function onIssueRequested() { dialog.open() }
    }
    Flickable {
        anchors.fill: parent
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        Column {
            id: body
            width: parent.width
            spacing: Metrics.gap16
            Item {
                width: parent.width
                height: Metrics.button28
                Text {
                    objectName: "reportIssueTitle"
                    anchors.left: parent.left
                    anchors.right: dismiss.left
                    anchors.rightMargin: Metrics.gap12
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Report an issue", I18n.rev)
                    font.pointSize: Type.section
                    font.weight: Type.semibold
                    color: Theme.color.textPrimary
                }
                IconButton {
                    id: dismiss
                    objectName: "reportIssueClose"
                    anchors.right: parent.right
                    tip: I18n.tr("Close", I18n.rev)
                    glyph: Icons.x
                    onClicked: dialog.close()
                }
            }
            Text {
                width: parent.width
                text: I18n.tr("Copy the redacted logs to help diagnose the issue. You can review them before sharing.", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.body
                color: Theme.color.textSecondary
            }
            Text {
                objectName: "reportIssuePluginNotice"
                width: parent.width
                text: I18n.tr("Report plugin errors in the corresponding plugin's repository.", I18n.rev)
                wrapMode: Text.WordWrap
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            Flow {
                width: parent.width
                spacing: Metrics.gap8
                PillButton {
                    objectName: "reportIssueGoToLogs"
                    text: I18n.tr("Go to logs", I18n.rev)
                    onClicked: logs.open()
                }
                PillButton {
                    objectName: "reportIssueReviewLogs"
                    text: I18n.tr("Review and copy logs…", I18n.rev)
                    onClicked: review.open()
                }
                PillButton {
                    objectName: "reportIssueCreate"
                    text: I18n.tr("Create issue", I18n.rev)
                    kind: "primary"
                    onClicked: { dialog.close(); dialog.openExternal(About.link("issue")) }
                }
            }
        }
    }
    DeveloperLogDialog { id: logs }
    DeveloperReportDialog {
        id: review
        objectName: "reportIssueReview"
        offerIssue: false
    }
}
