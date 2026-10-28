import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Modal {
    id: dialog
    property bool offerIssue: true
    width: Math.min(Metrics.settingsMaxWidth, parent.width - Metrics.modalInsets)
    height: Math.min(Metrics.panesHeight + Metrics.modalInsets, parent.height - Metrics.modalInsets)
    onAboutToShow: Developers.prepareReport()
    onClosed: Developers.closeReport()
    Column {
        id: header
        width: parent.width
        spacing: Metrics.gap8
        Text { text: I18n.tr("Review diagnostics", I18n.rev); font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
    }
    Flickable {
        objectName: "developerReportViewport"
        anchors.top: header.bottom
        anchors.topMargin: Metrics.gap16
        anchors.bottom: footer.top
        anchors.bottomMargin: Metrics.gap16
        width: parent.width
        contentHeight: report.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        TextEdit {
            id: report
            objectName: "developerReviewedReport"
            width: parent.width
            text: dialog.visible ? Developers.review : ""
            readOnly: true
            selectByMouse: true
            wrapMode: TextEdit.WrapAnywhere
            textFormat: TextEdit.PlainText
            font.pointSize: Type.meta
            color: Theme.color.textPrimary
        }
    }
    Column {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width
        spacing: Metrics.gap8
        Text { width: parent.width; text: Developers.status; visible: text !== ""; wrapMode: Text.WordWrap; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            PillButton { objectName: "developerCopyReport"; text: I18n.tr("Copy report", I18n.rev); onClicked: Developers.copyReport() }
            PillButton { objectName: "developerExportReport"; text: I18n.tr("Export…", I18n.rev); onClicked: Developers.exportReport() }
            PillButton { objectName: "developerOpenIssue"; visible: dialog.offerIssue; text: I18n.tr("Open issue draft", I18n.rev); onClicked: { dialog.close(); About.reportIssue() } }
            PillButton { objectName: "developerCloseReport"; text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
    }
}
