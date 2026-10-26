import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../qml/Patterns"

TestCase {
    name: "CastOutputMenu"
    when: windowShown
    visible: true
    width: 960
    height: 600
    Item { id: anchor; x: 900; y: 550; width: 28; height: 28; signal clicked(); onClicked: picker.show(this) }
    CastPicker { id: picker }

    function init() {
        Cast.active = false
        Cast.nextActive = null
        Cast.selected = ""
        Cast.disconnected = false
        Cast.snapshot = JSON.stringify({ entries: picker.previewEntries, footer: picker.previewFooter })
    }
    function cleanup() { picker.close(); wait(0) }
    function entry(action) {
        for (const item of picker.entryItems)
            if (item.entry !== undefined && item.entry.act === action) return item
        return null
    }
    function test_groups_width_anchor_and_device_dispatch() {
        anchor.clicked()
        tryCompare(picker, "opened", true)
        compare(Cast.picking, true)
        compare(picker.width, 270)
        verify(picker.y + picker.height <= anchor.y)
        verify(picker.x + picker.width <= width)
        compare(picker.entries[2].text, "Chromecast")
        compare(picker.entries[5].text, "DLNA / UPnP")
        const row = entry("connect")
        verify(row.width > 240)
        waitForRendering(row)
        mouseClick(row, row.width / 2, row.height / 2)
        compare(Cast.selected, "living")
        tryCompare(picker, "visible", false)
        compare(Cast.picking, false)
    }
    function test_local_output_disconnects() {
        picker.show(anchor)
        tryCompare(picker, "opened", true)
        const row = entry("local")
        waitForRendering(row)
        mouseClick(row, row.width / 2, row.height / 2)
        compare(Cast.disconnected, true)
        tryCompare(picker, "visible", false)
    }
    function test_fixed_details_footer_does_not_select_or_disconnect_a_renderer() {
        picker.show(anchor)
        tryCompare(picker,"opened",true)
        const footer = findChild(picker.contentItem,"menuFooter0")
        verify(footer !== null)
        footer.forceActiveFocus()
        keyClick(Qt.Key_Return)
        tryCompare(picker,"opened",false)
        compare(AudioOutput.shown,true)
        compare(AudioOutput.tab,"equipment")
        compare(Cast.selected,"")
        compare(Cast.disconnected,false)
        AudioOutput.close()
    }
    function test_long_list_stays_in_overlay_and_scrolls() {
        const entries = picker.previewEntries.slice()
        for (let i = 0; i < 30; ++i)
            entries.push({ act: "connect", id: "extra" + i, text: "Renderer " + i, detail: "Network receiver" })
        Cast.snapshot = JSON.stringify({ entries: entries, footer: picker.previewFooter })
        picker.show(anchor)
        tryCompare(picker, "opened", true)
        verify(picker.y >= 10)
        verify(picker.y + picker.height <= height - 10)
        tryVerify(() => picker.contentItem.contentHeight > picker.contentItem.height)
        picker.contentItem.contentY = picker.contentItem.contentHeight - picker.contentItem.height
        wait(0)
        verify(picker.contentItem.contentY > 0)
    }
}
