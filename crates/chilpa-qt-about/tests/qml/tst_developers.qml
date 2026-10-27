import QtQuick
import QtTest
import app.chilpa.about
import "qrc:/qt/qml/app/chilpa/about/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/about/qml/Components"

TestCase {
    id: test
    name: "Developers"
    when: windowShown
    visible: true
    width: 960
    height: 600
    property int backgroundClicks: 0
    MouseArea { anchors.fill: parent; onClicked: test.backgroundClicks++ }
    DeveloperSettings { id: panel; width: 650; height: 500; active: true }
    DeveloperLogDialog { id: logs }
    DeveloperReportDialog { id: report }
    SignalSpy { id: castSpy; target: panel; signalName: "castRequested" }
    function init() {
        test.width=960; test.height=600; panel.active=true; panel.expanded=false
        Developers.release(); Developers.refreshed=0; Developers.logsRefreshed=0; Developers.copied=0
        backgroundClicks=0; castSpy.clear()
    }
    function cleanup() { logs.close(); report.close(); panel.active=false; tryCompare(logs,"visible",false); tryCompare(report,"visible",false) }
    function click(item) { waitForRendering(item); mouseClick(item) }
    function test_diagnostics_are_manual_and_leave_no_closed_document() {
        compare(Developers.refreshed,0)
        click(findChild(panel,"developerDiagnostics"))
        compare(Developers.refreshed,1)
        click(findChild(panel,"developerRefreshDiagnostics"))
        compare(Developers.refreshed,2)
        click(findChild(panel,"developerScanCast"))
        compare(castSpy.count,1)
        wait(300); compare(Developers.refreshed,2)
        panel.active=false
        compare(Developers.diagnostics,"")
        compare(Developers.logs,"")
        compare(Developers.review,"")
    }
    function test_log_snapshot_virtualizes_and_releases_on_close() {
        logs.open(); tryCompare(logs,"opened",true)
        const list=findChild(logs.contentItem,"developerLogList")
        verify(list.height>0)
        compare(list.count,512)
        verify(list.contentItem.children.length<512)
        compare(Developers.logsRefreshed,1)
        wait(300); compare(Developers.logsRefreshed,1)
        logs.advanced=true
        click(findChild(logs.contentItem,"developerRefreshLogs"))
        compare(Developers.logsRefreshed,2)
        test.height=400
        tryVerify(()=>logs.height<=test.height)
        verify(list.height>0)
        mouseClick(test,955,395)
        compare(backgroundClicks,0)
        tryCompare(logs,"visible",false)
        compare(Developers.logs,"")
    }
    function test_copy_requires_a_reviewed_report_and_modal_blocks_background() {
        report.open(); tryCompare(report,"opened",true)
        compare(findChild(report.contentItem,"developerReviewedReport").text,Developers.review)
        compare(Developers.copied,0)
        click(findChild(report.contentItem,"developerCopyReport"))
        compare(Developers.copied,1)
        verify(findChild(report.contentItem,"developerReportViewport").height>0)
        mouseClick(test,955,595)
        compare(backgroundClicks,0)
        tryCompare(report,"visible",false)
        compare(Developers.review,"")
    }
}
