import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

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
    function init() {
        model.identities = "[5]"
        model.removed = []
    }
    function cleanup() { removal.close() }
    function test_playlist_removal_requires_one_confirmation() {
        removal.show(model,0)
        tryCompare(removal,"opened",true)
        compare(model.removed.length,0)
        findChild(removal.contentItem,"confirmationAccept").clicked()
        compare(model.removed.length,1)
        compare(model.removed[0],0)
        compare(removal.target,null)
        compare(removal.selection,"")
    }
    function test_a_different_selection_with_the_same_count_is_rejected() {
        removal.show(model,0)
        tryCompare(removal,"opened",true)
        model.identities = "[8]"
        findChild(removal.contentItem,"confirmationAccept").clicked()
        compare(model.removed.length,0)
        verify(removal.opened)
    }
}
