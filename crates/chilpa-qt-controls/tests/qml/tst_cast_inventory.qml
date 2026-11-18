import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

// The output menu draws the remembered renderers as the facade lists them
// and only says which entry was chosen: protocol and identifier travel apart.
TestCase {
    name: "CastInventoryMenu"
    when: windowShown
    visible: true
    width: 960
    height: 600
    Item { id: anchor; x: 900; y: 550; width: 28; height: 28 }
    CastPicker { id: picker }

    readonly property var listing: [
        { act: "local", icon: "local", text: "Desktop audio", detail: "This computer", checked: true },
        { separator: true }, { text: "Chromecast", note: true },
        { act: "connect", protocol: "cast", id: "shared", icon: "cast", text: "Kitchen", detail: "Receiver · Unavailable" },
        { separator: true }, { text: "DLNA / UPnP", note: true },
        { act: "connect", protocol: "upnp", id: "shared", icon: "dlna", text: "Hall", detail: "Renderer · Not checked yet" },
        { separator: true },
        { act: "discover", text: "Search for devices", keepOpen: true },
        { act: "forget-pick", text: "Forget a device…", keepOpen: true }
    ]
    readonly property var forgetting: [
        { text: "Forget a device", note: true },
        { act: "forget", protocol: "cast", id: "shared", icon: "cast", text: "Kitchen", detail: "Receiver", keepOpen: true },
        { act: "forget", protocol: "upnp", id: "shared", icon: "dlna", text: "Hall", detail: "Disconnect before forgetting this device.", enabled: false, keepOpen: true },
        { separator: true }, { act: "forget-done", text: "Back", keepOpen: true }
    ]
    function init() {
        Cast.active = false; Cast.selected = ""; Cast.disconnected = false; Cast.acts = []
        Cast.snapshot = JSON.stringify({ entries: listing, footer: [] })
    }
    function cleanup() { picker.close(); wait(0) }
    function row(text) {
        for (const item of picker.entryItems)
            if (item.entry !== undefined && item.entry.text === text) return item
        return null
    }
    function click(text) {
        const item = row(text)
        verify(item !== null, text)
        waitForRendering(item)
        mouseClick(item, item.width / 2, item.height / 2)
    }
    function last() { return Cast.acts.length ? Cast.acts[Cast.acts.length - 1].join("|") : "" }

    function test_remembered_rows_show_on_opening_with_their_presence() {
        picker.show(anchor)
        tryCompare(picker, "opened", true)
        compare(Cast.picking, true)
        compare(row("Kitchen").entry.detail, "Receiver · Unavailable")
        compare(row("Hall").entry.detail, "Renderer · Not checked yet")
        compare(Cast.acts.length, 0)
    }
    function test_searching_keeps_the_menu_open_and_connects_nothing() {
        picker.show(anchor)
        tryCompare(picker, "opened", true)
        click("Search for devices")
        compare(last(), "discover||")
        compare(picker.opened, true)
        compare(Cast.selected, "")
        compare(Cast.disconnected, false)
    }
    function test_a_row_is_chosen_by_protocol_and_identifier() {
        picker.show(anchor)
        tryCompare(picker, "opened", true)
        click("Hall")
        compare(last(), "connect|upnp|shared")
        tryCompare(picker, "visible", false)
        compare(Cast.picking, false)
        picker.show(anchor)
        tryCompare(picker, "opened", true)
        click("Kitchen")
        compare(last(), "connect|cast|shared")
    }
    function test_forgetting_lists_devices_and_leaves_the_one_in_use() {
        picker.show(anchor)
        tryCompare(picker, "opened", true)
        click("Forget a device…")
        compare(last(), "forget-pick||")
        compare(picker.opened, true)
        Cast.snapshot = JSON.stringify({ entries: forgetting, footer: [] })
        tryVerify(() => row("Back") !== null)
        const count = Cast.acts.length
        click("Hall")
        compare(Cast.acts.length, count)
        compare(row("Hall").entry.detail, "Disconnect before forgetting this device.")
        click("Kitchen")
        compare(last(), "forget|cast|shared")
        compare(picker.opened, true)
        compare(Cast.selected, "")
    }
}
