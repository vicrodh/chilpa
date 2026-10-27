import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Modal {
    id: dialog
    signal shareRequested()
    property bool advanced: false
    readonly property var document: JSON.parse(Developers.logs || "{}")
    width: Math.min(Metrics.settingsMaxWidth, parent.width - Metrics.modalInsets)
    height: Math.min(Metrics.panesHeight + Metrics.modalInsets, parent.height - Metrics.modalInsets)
    function refresh() { Developers.refreshLogs(level.value || "all", search.text) }
    onOpened: { advanced = false; level.value = "all"; search.text = ""; refresh() }
    onClosed: Developers.releaseLogs()
    Column {
        id: header
        width: parent.width
        spacing: Metrics.gap12
        Row {
            spacing: Metrics.gap12
            Text { text: I18n.tr("Application logs", I18n.rev); font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
            Text { text: (dialog.document.shown || 0) + " / " + (dialog.document.total || 0); font.pointSize: Type.meta; color: Theme.color.textSecondary }
            IconButton { tip: I18n.tr("Log options", I18n.rev); glyph: dialog.advanced ? Icons.minus : Icons.plus; onClicked: dialog.advanced = !dialog.advanced }
        }
        Flow {
            visible: dialog.advanced
            width: parent.width
            spacing: Metrics.gap8
            SelectField {
                id: level
                width: Metrics.selectWidth / 2 // derived: compact severity selector
                options: [I18n.tr("All levels", I18n.rev), I18n.tr("Error", I18n.rev), I18n.tr("Warning", I18n.rev), I18n.tr("Info", I18n.rev), "stderr"]
                values: ["all","error","warn","info","stderr"]
                value: "all"
                onPicked: value => { level.value = value; dialog.refresh() }
            }
            TextField { id: search; placeholder: I18n.tr("Search logs", I18n.rev); onAccepted: dialog.refresh() }
            PillButton { objectName: "developerRefreshLogs"; text: I18n.tr("Refresh", I18n.rev); onClicked: dialog.refresh() }
            PillButton { text: I18n.tr("Clear logs", I18n.rev); onClicked: { Developers.clearLogs(); dialog.refresh() } }
            PillButton { text: I18n.tr("Open log file", I18n.rev); enabled: dialog.document.fileAvailable === true; onClicked: Qt.openUrlExternally(Developers.logUrl()) }
        }
        DeveloperLogRow {
            width: parent.width
            heading: true
            entry: ({time:I18n.tr("Time", I18n.rev),level:I18n.tr("Level", I18n.rev),target:I18n.tr("Target", I18n.rev),message:I18n.tr("Message", I18n.rev)})
        }
    }
    ListView {
        objectName: "developerLogList"
        anchors.top: header.bottom
        anchors.bottom: footer.top
        anchors.bottomMargin: Metrics.gap16
        width: parent.width
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        model: dialog.visible ? dialog.document.rows || [] : []
        delegate: DeveloperLogRow { required property var modelData; width: ListView.view.width; entry: modelData }
        ScrollBar.vertical: ViewportScrollBar {}
    }
    Column {
        id: footer
        anchors.bottom: parent.bottom
        width: parent.width
        spacing: Metrics.gap8
        Text { width: parent.width; text: dialog.document.status || ""; visible: text !== ""; wrapMode: Text.WordWrap; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            PillButton { objectName: "developerShareLogs"; text: I18n.tr("Review and share…", I18n.rev); onClicked: dialog.shareRequested() }
            PillButton { text: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
    }
}
