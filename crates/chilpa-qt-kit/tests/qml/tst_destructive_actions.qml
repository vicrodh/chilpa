import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Controls

TestCase {
    id: test
    name: "DestructiveActions"
    when: windowShown
    visible: true
    width: 800
    height: 700
    QtObject {
        id: model
        property string kind: "playlist"
        property int albumId: 7
        property string key: ""
        property string order: "custom"
        property bool ascending: true
        property int count: 2
        property int filterRev: 0
        property int selectedCount: 1
        property string identities: "[5]"
        property var removed: []
        function chosenTrackIdsJson(row) { return identities }
        function removeChosen(row) { removed = removed.concat([row]) }
    }
    ModelActionConfirmation { id: removal }
    Controls.QueueClearDialog { id: clearing }
    function init() {
        model.identities = "[5]"
        model.removed = []
        ListenList.snapshot = JSON.stringify({active:"1",selected:"1",tabs:[{id:"1",name:"First"},{id:"2",name:"Second"}]})
        ListenList.lastAction = ""
        ListenList.lastId = ""
    }
    function cleanup() { removal.close(); clearing.close() }
    function acceptFirst(question) {
        tryCompare(question,"opened",true)
        findChild(question.contentItem,"confirmationAccept").clicked()
        findChild(question.contentItem,"destructiveAcknowledgement").checked = true
    }
    function test_the_list_command_runs_once_after_the_second_confirmation() {
        removal.show(model,0,"remove",false,"")
        acceptFirst(removal)
        compare(model.removed.length,0)
        findChild(removal.contentItem,"confirmationAccept").clicked()
        compare(model.removed.length,1)
        compare(model.removed[0],0)
        compare(removal.target,null)
        compare(removal.selection,"")
    }
    function test_a_different_selection_with_the_same_count_is_rejected() {
        removal.show(model,0,"remove",false,"")
        acceptFirst(removal)
        model.identities = "[8]"
        findChild(removal.contentItem,"confirmationAccept").clicked()
        compare(model.removed.length,0)
        verify(removal.opened)
    }
    function test_clearing_keeps_the_original_queue_when_selection_changes() {
        clearing.show("1",false)
        acceptFirst(clearing)
        const document = JSON.parse(ListenList.snapshot)
        document.selected = "2"
        ListenList.snapshot = JSON.stringify(document)
        compare(ListenList.lastAction,"")
        findChild(clearing.contentItem,"confirmationAccept").clicked()
        compare(ListenList.lastAction,"clear")
        compare(ListenList.lastId,"1")
    }
    function test_a_live_queue_replacement_requires_a_new_confirmation() {
        clearing.show("",true)
        acceptFirst(clearing)
        const document = JSON.parse(ListenList.snapshot)
        document.active = "2"
        ListenList.snapshot = JSON.stringify(document)
        tryCompare(clearing,"targetValid",false)
        findChild(clearing.contentItem,"confirmationAccept").clicked()
        compare(ListenList.lastAction,"")
        verify(clearing.opened)
    }
}
