import QtQuick
import QtTest
import app.chilpa.kit
import "../../qml/Patterns"

TestCase {
    name: "CollectionActions"
    when: windowShown
    visible: true
    width: 760
    height: 520

    QtObject {
        id: collection
        property var requests: []
        function queueCollection(action, shuffled, limit, label) {
            requests = requests.concat([[action, shuffled, limit, label]])
        }
    }
    CollectionPlayback {
        id: actions
        x: 20
        y: 20
        availableWidth: parent.width - 40
        model: collection
        available: true
        filtered: false
        scope: "all"
        label: "Albums"
    }

    function init() {
        collection.requests = []
        actions.filtered = false
        actions.scope = "all"
        actions.available = true
        actions.label = "Albums"
        actions.showRandomAlbum = false
        QueueActivity.pending = false
        QueueActivity.refresh()
        failOnWarning(/TypeError|ReferenceError|Binding loop/)
    }

    function chooser() { return findChild(actions, "collectionSizeDialog") }

    function test_unfiltered_shuffle_waits_for_the_size_to_be_chosen() {
        mouseClick(findChild(actions, "collectionShuffle"))
        tryCompare(chooser(), "opened", true)
        compare(collection.requests.length, 0)
        const size = findChild(chooser().contentItem, "collectionSizeSelect")
        compare(size.values, ["50", "100", "200", "500", "1000", "0", "custom"])
        mouseClick(size)
        const options = findChild(size, "selectOptions")
        tryCompare(options, "opened", true)
        options.choose(0, 0)
        compare(collection.requests.length, 0, "choosing a size does not start playback")
        mouseClick(findChild(chooser().contentItem, "collectionSizeConfirm"))
        compare(collection.requests, [["play", true, 50, "Albums"]])
        tryCompare(chooser(), "opened", false)
    }

    function test_shuffle_progress_keeps_the_dialog_open_until_the_queue_finishes() {
        mouseClick(findChild(actions, "collectionShuffle"))
        tryCompare(chooser(), "opened", true)
        const dialog = chooser()
        const confirm = findChild(dialog.contentItem, "collectionSizeConfirm")
        const progress = findChild(dialog.contentItem, "collectionQueueProgress")
        const height = dialog.height
        QueueActivity.pending = true
        mouseClick(confirm)
        tryCompare(progress, "indeterminate", true)
        compare(dialog.opened, true)
        compare(dialog.height, height)
        compare(confirm.enabled, false)
        keyClick(Qt.Key_Escape)
        verify(dialog.opened)
        compare(collection.requests, [["play", true, 50, "Albums"]])
        QueueActivity.pending = false
        QueueActivity.refresh()
        tryCompare(dialog, "opened", false)
    }

    function test_random_album_uses_one_album_directly_even_without_filters() {
        const random = findChild(actions, "collectionRandomAlbum")
        verify(!random.visible)
        actions.showRandomAlbum = true
        verify(random.visible)
        const shuffle = findChild(actions, "collectionShuffle")
        tryVerify(() => random.x > shuffle.x)
        mouseClick(random)
        compare(chooser().opened, false)
        compare(collection.requests, [["random-album", false, 0, "Albums"]])
    }

    function test_custom_size_reserves_space_and_requires_a_positive_integer() {
        mouseClick(findChild(actions, "collectionPlay"))
        tryCompare(chooser(), "opened", true)
        const size = findChild(chooser().contentItem, "collectionSizeSelect")
        const custom = findChild(chooser().contentItem, "collectionCustomSize")
        const confirm = findChild(chooser().contentItem, "collectionSizeConfirm")
        const height = chooser().height
        const buttonY = confirm.mapToItem(chooser().contentItem, 0, 0).y
        verify(!custom.visible)
        size.picked("custom")
        verify(custom.visible)
        verify(!confirm.enabled)
        compare(chooser().height, height)
        compare(confirm.mapToItem(chooser().contentItem, 0, 0).y, buttonY)
        custom.text = "2147483648"
        verify(!confirm.enabled)
        custom.text = "137"
        verify(confirm.enabled)
        mouseClick(confirm)
        compare(collection.requests, [["play", false, 137, "Albums"]])
        mouseClick(findChild(actions, "collectionPlay"))
        tryCompare(chooser(), "opened", true)
        compare(findChild(chooser().contentItem, "collectionSizeSelect").value, "50")
        chooser().close()
    }

    function test_filtered_play_uses_the_entire_result_without_opening_the_dialog() {
        actions.label = "Tracks"
        actions.filtered = true
        mouseClick(findChild(actions, "collectionPlay"))
        compare(chooser().opened, false)
        compare(collection.requests, [["play", false, 0, "Tracks"]])
    }

    function test_a_new_scope_withdraws_the_confirmation() {
        mouseClick(findChild(actions, "collectionPlay"))
        tryCompare(chooser(), "opened", true)
        actions.scope = "changed"
        tryCompare(chooser(), "opened", false)
        compare(collection.requests.length, 0)
    }

    function test_queue_menu_items_use_the_same_size_selection() {
        const button = findChild(actions, "collectionQueue")
        mouseClick(button)
        const popup = findChild(actions, "collectionQueueMenu")
        verify(popup !== null)
        tryCompare(popup, "opened", true)
        popup.actionChosen({act: "next", text: "Play next"})
        popup.close()
        tryCompare(chooser(), "opened", true)
        const select = findChild(chooser().contentItem, "collectionSizeSelect")
        select.picked("1000")
        mouseClick(findChild(chooser().contentItem, "collectionSizeConfirm"))
        compare(collection.requests, [["next", false, 1000, "Albums"]])
    }
}
