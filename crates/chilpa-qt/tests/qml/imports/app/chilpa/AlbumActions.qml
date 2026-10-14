pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property int selectedCount: 0
    property var informationDocument: ({})
    function information(id, revision) { return JSON.stringify(informationDocument) }
    function artistInformation(id, revision) { return JSON.stringify(informationDocument) }
    property var artistDestination: ({})
    function sources(id, revision) { return "[]" }
    function quality(id, revision) { return '{"tier":3,"label":"FLAC 24/96"}' }
    function available(id, revision) { return true }
    function selected(id, revision) { return false }
    property double playedId: 0
    function play(id, shuffled) { playedId = id }
    function queue(id) {}
    function playNext(id) { lastAction = "next:" + id }
    function playLater(id, label) { lastAction = "later:" + id + ":" + label }
    property string lastAction: ""
    function listenTargetsJson() { return JSON.stringify([{separator:true},{text:"Extra queues",submenu:true,children:[{act:"buffer:new",text:"Open in new queue"},{act:"buffer:add:2",text:"Add to queue Two"},{act:"buffer:replace:2",text:"Replace queue Two"}]}]) }
    function listAction(id, action) { lastAction = action + ":" + id }
    function playMany(idsJson, shuffled) { lastAction = "many:" + idsJson }
    function trackIdsJson(id) { return "[]" }
}
