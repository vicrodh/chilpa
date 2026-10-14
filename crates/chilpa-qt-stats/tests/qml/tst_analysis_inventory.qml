import QtQuick
import QtTest
import app.chilpa.stats
import "../../qml/Patterns"

TestCase {
    name: "AnalysisInventory"
    when: windowShown
    visible: true
    width: 1200; height: 900
    Component { id: inventory; AnalysisInventoryDialog {} }
    function document() {
        return { total:3, standard:2, mood:1, matched:3, running:true, active:[10],
            runs:[{ id:10, kind:"mood", started_at:1780000000, finished_at:0, state:"running", previous:1, processed:1, total:2, saved:1, failed:0, error:"", folder:"", last:"Second track" }],
            tracks:[{id:1,title:"First track",artist:"Artist",album:"Record",standard:true,mood:true,standard_at:1770000000,mood_at:1770000000},
                {id:2,title:"Second track",artist:"Artist",album:"Record",standard:true,mood:false,standard_at:1770000000},
                {id:3,title:"Third track",artist:"Artist",album:"Record",standard:false,mood:false}]
        }
    }
    function init() { Stats.inventoryDocument = document() }
    function open() {
        const dialog = createTemporaryObject(inventory, this)
        dialog.show(); tryCompare(dialog, "opened", true)
        return dialog
    }
    function test_the_default_bridge_populates_saved_tracks() {
        const dialog = open(), list = findChild(dialog, "inventoryTrackList")
        compare(dialog.error, "")
        compare(list.count, 3)
        tryVerify(() => list.itemAtIndex(0) !== null)
        compare(list.itemAtIndex(0).entry.standard, true)
        verify(list.itemAtIndex(0).height > 0)
        verify(list.height > 0)
    }
    function test_live_results_update_the_same_visible_rows() {
        const dialog = open(), list = findChild(dialog, "inventoryTrackList")
        tryVerify(() => list.itemAtIndex(0) !== null)
        const first = list.itemAtIndex(0), next = document()
        next.mood = 2; next.runs[0].processed = 2; next.runs[0].saved = 2; next.tracks[1].mood = true; next.tracks[1].mood_at = 1780000000
        Stats.inventoryDocument = next
        tryVerify(() => dialog.summary.mood === 2, 2200)
        compare(list.itemAtIndex(0), first)
        compare(list.itemAtIndex(1).entry.mood, true)
    }
    function test_history_opens_the_tracks_of_the_selected_run() {
        const dialog = open()
        findChild(dialog, "inventoryTabs").activated(1)
        const history = findChild(dialog, "inventoryRunList")
        tryVerify(() => history.itemAtIndex(0) !== null)
        compare(history.count, 1)
        history.itemAtIndex(0).asked()
        compare(dialog.runId, 10)
        compare(dialog.history, false)
        findChild(dialog, "inventoryAllRuns").clicked()
        compare(dialog.runId, 0)
    }
    function test_a_read_failure_keeps_the_existing_inventory_visible() {
        const dialog = open()
        Stats.inventoryDocument = { error:"A read failed" }
        dialog.fetch(false, true)
        compare(dialog.error, "A read failed")
        compare(findChild(dialog, "inventoryTrackList").count, 3)
    }
}
