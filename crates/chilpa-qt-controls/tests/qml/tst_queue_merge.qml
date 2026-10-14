import QtQuick
import QtTest
import app.chilpa.controls
import "../../qml/Patterns"

TestCase {
    name: "QueueMerge"
    when: windowShown
    visible: true
    width: 900; height: 700
    QueueMergeDialog { id: dialog }
    MouseArea { id: behind; anchors.fill: parent; onClicked: testCase.backgroundClicks++ }
    property int backgroundClicks: 0
    id: testCase

    function descendants(item) {
        let result = [item]
        for (const child of item.children || []) result = result.concat(descendants(child))
        return result
    }
    function choice(name) { return descendants(dialog.contentItem).find(item => item.objectName === "queueMergeChoice" && item.text.startsWith(name)) }
    function init() {
        backgroundClicks = 0; Queues.merged = []; Queues.error = ""
        dialog.document = {selected:"2", tabs:[{id:"1",name:"First",count:3},{id:"2",name:"Second",count:2},{id:"3",name:"Third",count:4}]}
        dialog.show(); tryVerify(() => dialog.opened); waitForRendering(dialog.contentItem)
    }
    function cleanup() { dialog.close(); tryVerify(() => !dialog.opened) }
    function test_combining_keeps_originals_by_default_and_respects_the_chosen_order() {
        compare(dialog.selectedCount, 1)
        const first = choice("First")
        mouseClick(first, first.width / 3, first.height / 2)
        compare(dialog.selectedCount, 2); compare(dialog.trackCount, 5)
        dialog.move(1, -1)
        findChild(dialog, "mergedQueueName").text = "Road trip"
        mouseClick(findChild(dialog, "confirmQueueMerge"))
        compare(Queues.merged, [["2", "1"], "Road trip", true])
        tryVerify(() => !dialog.opened)
    }
    function test_originals_are_removed_only_when_chosen_in_this_dialog() {
        mouseClick(choice("Third"))
        const selector = findChild(dialog, "mergedQueueOriginals")
        selector.picked("remove")
        dialog.combine()
        compare(Queues.merged[2], false)
        dialog.show(); tryVerify(() => dialog.opened)
        compare(selector.value, "keep", "every combination asks again")
    }
    function test_live_queue_removal_updates_the_picker_without_selecting_another_queue() {
        mouseClick(choice("First"))
        dialog.document = {selected:"2", tabs:[{id:"2",name:"Second",count:7},{id:"3",name:"Third",count:4}]}
        tryCompare(dialog, "selectedCount", 1)
        compare(dialog.trackCount, 7)
        verify(!findChild(dialog, "confirmQueueMerge").enabled)
    }
    function test_errors_keep_the_selection_and_dialog_open() {
        mouseClick(choice("First")); Queues.error = "A selected queue is no longer available."
        dialog.combine(); compare(dialog.error, Queues.error); verify(dialog.opened)
        compare(dialog.selectedCount, 2)
    }
    function test_clicks_inside_the_modal_do_not_reach_the_background() {
        mouseClick(dialog.contentItem, dialog.contentItem.width - 1, 1)
        compare(backgroundClicks, 0); verify(dialog.opened)
    }
}
