import QtQuick
import QtTest
import app.chilpa.sources
import "../../qml/Patterns"

TestCase {
    name: "ServerMusicDeletion"
    when: windowShown
    visible: true
    width: 700; height: 700
    ServerMediaDeletion { id: host }
    ServerMediaDeleteDialog { id: dialog; kind: "track"; localId: 7 }
    function init() {
        Sources.mediaInfo = {title:"A <b>track</b>",server:"Music server",address:"http://fixture.invalid",source:2,ref:"item-id"}
        Sources.mediaCalls = []
        Sources.mediaReceipt = {request_id:"media-test",pending:true}
    }
    function cleanup() { dialog.close() }
    function test_opening_and_cancelling_never_deletes_and_metadata_is_plain_text() {
        dialog.open()
        tryCompare(dialog,"opened",true)
        const title = findChild(dialog.contentItem,"serverDeleteTitle")
        compare(title.text,"A <b>track</b>")
        compare(title.textFormat,Text.PlainText)
        compare(Sources.mediaCalls.length,0)
        mouseClick(findChild(dialog.contentItem,"serverDeleteCancel"))
        tryCompare(dialog,"visible",false)
        compare(Sources.mediaCalls.length,0)
    }
    function test_confirming_deletes_once_and_waits_for_the_result() {
        dialog.open()
        tryCompare(dialog,"opened",true)
        mouseClick(findChild(dialog.contentItem,"serverDeleteConfirm"))
        compare(Sources.mediaCalls.length,1)
        compare(JSON.stringify(Sources.mediaCalls[0]),JSON.stringify(["track",7,"item-id",2]))
        compare(dialog.busy,true)
        keyClick(Qt.Key_Escape)
        verify(dialog.visible,"the confirmed operation keeps its progress visible")
        Sources.mediaReceipt = {request_id:"media-test",pending:false,done:true}
        tryVerify(() => dialog.receipt.done === true)
        compare(Sources.mediaCalls.length,1)
        mouseClick(findChild(dialog.contentItem,"serverDeleteCancel"))
        tryCompare(dialog,"visible",false)
    }
    function test_a_menu_request_mounts_one_dialog_and_close_releases_it() {
        Sources.serverMediaRequested("track",7)
        tryVerify(() => findChild(host,"serverMediaDeleteDialog") !== null)
        const mounted = findChild(host,"serverMediaDeleteDialog")
        tryCompare(mounted,"opened",true)
        mouseClick(findChild(mounted.contentItem,"serverDeleteCancel"))
        tryCompare(host,"requested",null)
        tryVerify(() => findChild(host,"serverMediaDeleteDialog") === null)
        compare(Sources.mediaCalls.length,0)
    }
    function test_an_item_without_a_server_identity_cannot_be_confirmed() {
        Sources.mediaInfo = {}
        dialog.open()
        tryCompare(dialog,"opened",true)
        verify(!findChild(dialog.contentItem,"serverDeleteConfirm").enabled)
        compare(Sources.mediaCalls.length,0)
    }
}
