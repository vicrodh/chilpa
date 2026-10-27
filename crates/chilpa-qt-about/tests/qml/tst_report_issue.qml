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
    function nestedReview() { return findChild(invitation.contentItem,"reportIssueReview") }
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
        tryCompare(review,"opened",true)
        verify(invitation.visible)
        compare(review.offerIssue,false)
        verify(!findChild(review.contentItem,"developerOpenIssue").visible)
        compare(Developers.copied,0)
        compare(externalUrls.length,0)
        click(findChild(review.contentItem,"developerCopyReport"))
        compare(Developers.copied,1)
        keyClick(Qt.Key_Escape)
        tryCompare(review,"visible",false)
        verify(invitation.visible)
        compare(Developers.review,"")
        click(findChild(invitation.contentItem,"reportIssueCreate"))
        tryCompare(invitation,"visible",false)
        compare(externalUrls.length,1)
        compare(externalUrls[0],About.link("issue"))
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
