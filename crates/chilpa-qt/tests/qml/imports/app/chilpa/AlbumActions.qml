pragma Singleton
import QtQuick
QtObject {
    property int rev: 0
    property int selectedCount: 0
    property var informationDocument: ({})
    function information(id, revision) { return JSON.stringify(informationDocument) }
    function artistInformation(id, revision) { return JSON.stringify(informationDocument) }
    property var artistDestination: ({})
    // What a test says a record's card stands for, by its id.
    property var sourcesOf: ({})
    function sources(id, revision) { return JSON.stringify(sourcesOf[id] || []) }
    function quality(id, revision) { return '{"tier":3,"label":"FLAC 24/96"}' }
    property var availabilityOf: ({})
    function available(id, revision) { return availabilityOf[id] !== false }
    // What a test says a record holds, for a list's columns, by its id.
    property var factsOf: ({})
    function facts(id, revision) { return JSON.stringify(factsOf[id] || {}) }
    property var technicalOf: ({})
    function technical(id, revision) { return JSON.stringify(technicalOf[id] || {}) }
    property var selectedIds: []
    function selected(id, revision) { return selectedIds.indexOf(id) >= 0 }
    property double playedId: 0
    function play(id, shuffled) { playedId = id }
    function queue(id) {}
    function playNext(id) { lastAction = "next:" + id }
    function playLater(id, label) { lastAction = "later:" + id + ":" + label }
    property string lastAction: ""
    function listenTargetsJson() { return JSON.stringify([{separator:true},{text:"Extra queues",submenu:true,children:[{act:"buffer:new",text:"Open in new queue"},{act:"buffer:add:2",text:"Add to queue Two"},{act:"buffer:replace:2",text:"Replace queue Two"}]}]) }
    function listAction(id, action) { lastAction = action + ":" + id }
    function playMany(idsJson, shuffled) { lastAction = "many:" + idsJson }
    property string menuTrackIdsJson: "[]"
    property int requestedAlbumId: 0
    function deviceAvailable(id, revision) { return (id === 0 || selectedIds.indexOf(id) >= 0 ? selectedIds : [id]).some(value => value > 0) }
    function deviceTracksJson(id) { return trackIdsJson(id) }
    function clearSelection() { selectedIds = []; selectedCount = 0; ++rev }
    function trackIdsJson(id) { requestedAlbumId = id; return menuTrackIdsJson }
}
