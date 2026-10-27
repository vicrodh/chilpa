import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

Item {
    id: panel
    property bool active: false
    property bool expanded: false
    signal castRequested(var anchor)
    readonly property var document: JSON.parse(Developers.diagnostics || "{}")
    onActiveChanged: if (!active) { expanded = false; logs.close(); report.close(); Developers.release() }
    Component.onDestruction: Developers.release()
    Flickable {
        anchors.fill: parent
        contentHeight: content.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.pageGutter }
        Column {
            id: content
            width: parent.width
            spacing: Metrics.gap16
            Text { text: I18n.tr("Application logs", I18n.rev); font.pointSize: Type.body; font.weight: Type.medium; color: Theme.color.textPrimary }
            Row {
                spacing: Metrics.gap8
                PillButton { objectName: "developerOpenLogs"; text: I18n.tr("View logs", I18n.rev); onClicked: logs.open() }
                PillButton { text: I18n.tr("Open log file", I18n.rev); enabled: panel.active && Developers.logUrl() !== ""; onClicked: Qt.openUrlExternally(Developers.logUrl()) }
            }
            PillButton {
                objectName: "developerDiagnostics"
                text: I18n.tr("Diagnostics", I18n.rev)
                glyph: panel.expanded ? Icons.chevronUp : Icons.chevronDown
                kind: "ghost"
                onClicked: { panel.expanded = !panel.expanded; if (panel.expanded) Developers.refresh(); else Developers.release() }
            }
            Column {
                width: parent.width
                visible: panel.expanded
                spacing: Metrics.gap12
                Flow {
                    width: parent.width
                    spacing: Metrics.gap8
                    PillButton { objectName: "developerRefreshDiagnostics"; text: I18n.tr("Refresh", I18n.rev); onClicked: Developers.refresh() }
                    PillButton { text: I18n.tr("Review and export…", I18n.rev); onClicked: report.open() }
                    PillButton { id: castButton; objectName: "developerScanCast"; text: I18n.tr("Scan for cast devices…", I18n.rev); onClicked: panel.castRequested(castButton) }
                }
                Repeater {
                    model: panel.expanded ? panel.document.sections || [] : []
                    DeveloperDiagnosticSection { required property var modelData; width: content.width; entry: modelData }
                }
            }
        }
    }
    DeveloperLogDialog { id: logs; onShareRequested: report.open() }
    DeveloperReportDialog { id: report }
}
