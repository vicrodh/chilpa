import QtQuick
import QtTest
import app.chilpa.about
import "qrc:/qt/qml/app/chilpa/about/qml/Components"

TestCase {
    id: test
    name: "ReportIssue"
    when: windowShown
    visible: true
    width: 960
    height: 600
    property var externalUrls: []
    property int backgroundClicks: 0
    MouseArea { anchors.fill: parent; onClicked: test.backgroundClicks++ }
    AboutBody { id: aboutBody; width: 650; doc: ({links:[]}) }
    DeveloperReportDialog { id: standalone }
    Item { id: pendingOverlay }
    Component { id: pendingFactory; ReportIssueDialog { parent: pendingOverlay; listenForRequests: false } }
    Component { id: windowFactory; Window { width: 960; height: 600; ReportIssueDialog { listenForRequests: false } } }
    ReportIssueDialog {
        id: invitation
        openExternal: function(url) { test.externalUrls.push(url) }
    }
    function init() {
        externalUrls=[]; backgroundClicks=0
        Developers.release(); Developers.copied=0
    }
    function cleanup() {
        standalone.close(); invitation.close()
        tryCompare(standalone,"visible",false); tryCompare(invitation,"visible",false)
    }
    function click(item) { waitForRendering(item); mouseClick(item) }
    function nestedReview() { return findChild(invitation,"reportIssueReview") }
    function test_a_new_window_does_not_measure_auxiliary_overlays() {
        failOnWarning(/Binding loop/)
        const window = createTemporaryObject(windowFactory, null)
        verify(window !== null)
        window.show()
        wait(50)
        window.close()
    }
    function test_nested_overlays_do_not_measure_the_invitation_before_layout() {
        failOnWarning(/Binding loop/)
        pendingOverlay.width = 0
        pendingOverlay.height = 0
        const dialog = createTemporaryObject(pendingFactory, test)
        verify(dialog !== null)
        compare(dialog.review.item, null)
        compare(dialog.logs.item, null)
        pendingOverlay.width = test.width
        pendingOverlay.height = test.height
        dialog.open()
        tryCompare(dialog, "opened", true)
        verify(dialog.height > 0 && dialog.height <= pendingOverlay.height)
        compare(Developers.review, "")
        compare(Developers.copied, 0)
        dialog.close()
    }
    function test_request_invites_without_collecting_or_sending_logs() {
        About.reportIssue()
        tryCompare(invitation,"opened",true)
        compare(externalUrls.length,0)
        compare(Developers.review,"")
        compare(Developers.copied,0)
        verify(findChild(invitation.contentItem,"reportIssuePluginNotice").text.indexOf("plugin")>=0)
        verify(invitation.width<=test.width && invitation.height<=test.height)
        mouseClick(test,955,595)
        tryCompare(invitation,"visible",false)
        compare(backgroundClicks,0)
        compare(externalUrls.length,0)
    }
    function test_about_entry_uses_the_same_invitation() {
        click(findChild(aboutBody,"aboutReportIssue"))
        tryCompare(invitation,"opened",true)
        compare(externalUrls.length,0)
        click(findChild(invitation.contentItem,"reportIssueClose"))
        tryCompare(invitation,"visible",false)
    }
    function test_review_and_copy_returns_to_invitation_before_create() {
        About.reportIssue(); tryCompare(invitation,"opened",true)
        click(findChild(invitation.contentItem,"reportIssueReviewLogs"))
        const review=nestedReview()
        verify(review!==null)
        compare(review.parent, invitation.parent)
        tryCompare(review,"opened",true)
        verify(invitation.visible)
        compare(review.offerIssue,false)
        verify(!findChild(review.contentItem,"developerOpenIssue").visible)
        compare(Developers.copied,0)
        compare(externalUrls.length,0)
        click(findChild(review.contentItem,"developerCopyReport"))
        compare(Developers.copied,1)
        keyClick(Qt.Key_Escape)
        tryVerify(() => invitation.review.item === null)
        verify(invitation.visible)
        compare(Developers.review,"")
        click(findChild(invitation.contentItem,"reportIssueCreate"))
        tryCompare(invitation,"visible",false)
        compare(externalUrls.length,1)
        compare(externalUrls[0],About.link("issue"))
    }
    function test_auxiliary_dialogs_release_on_close_and_can_reopen() {
        About.reportIssue()
        tryCompare(invitation, "opened", true)
        for (let cycle = 0; cycle < 2; ++cycle) {
            click(findChild(invitation.contentItem, "reportIssueGoToLogs"))
            tryVerify(() => invitation.logs.item && invitation.logs.item.opened)
            compare(invitation.logs.item.parent, invitation.parent)
            invitation.logs.item.close()
            tryVerify(() => invitation.logs.item === null)
            click(findChild(invitation.contentItem, "reportIssueReviewLogs"))
            tryVerify(() => invitation.review.item && invitation.review.item.opened)
            click(findChild(invitation.review.item.contentItem, "developerCloseReport"))
            tryVerify(() => invitation.review.item === null)
            verify(invitation.opened)
        }
        compare(externalUrls.length, 0)
    }
    function test_developer_report_routes_through_invitation() {
        standalone.open(); tryCompare(standalone,"opened",true)
        click(findChild(standalone.contentItem,"developerOpenIssue"))
        tryCompare(standalone,"visible",false)
        tryCompare(invitation,"opened",true)
        compare(externalUrls.length,0)
        compare(Developers.review,"")
    }
}
