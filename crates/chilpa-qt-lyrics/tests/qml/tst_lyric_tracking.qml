import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

TestCase {
    id: test
    name: "LyricTracking"
    when: windowShown
    visible: true
    width: 460
    height: 650
    LyricLines { id: verses; width: 420; height: 600; lite: true }
    Component { id: temporary; LyricLines { width: 420; height: 600; synced: true; follow: false; lines: [{text:"A verse",start_ms:0}] } }
    function document() {
        return Array.from({length:30}, (_, i) => ({text:"A line with a changing translation " + i,start_ms:i * 3000}))
    }
    function translated(words) {
        const result = {}
        for (let i = 0; i < 30; ++i) result[i] = Array.from({length:words}, (_, n) => "translated").join(" ")
        return result
    }
    function centered() {
        verses.forceLayout()
        const row = verses.itemAtIndex(verses.current)
        return row !== null && Math.abs(row.y - verses.contentY + row.height / 2 - verses.height / 2) <= 1
    }
    function settled() { tryVerify(centered, 1500, "the active row stays centered after text and layout settle") }
    function init() {
        Motion.reduced = false
        verses.cancelFlick()
        verses.width = 420; verses.height = 600; verses.size = Type.lyric
        verses.translation = null
        verses.synced = true; verses.follow = true; verses.current = 12
        verses.lines = document()
        settled()
    }
    function cleanup() { Motion.reduced = false }
    function test_late_wrapped_translation_and_romanization_keep_the_active_line_centered() {
        verses.translation = translated(28)
        settled()
        verify(verses.follow)
        verses.translation = translated(8)
        settled()
        verses.translation = null
        settled()
        verify(verses.follow, "changing a layer never turns off follow")
    }
    function test_width_font_and_delegate_height_changes_keep_the_anchor() {
        verses.translation = translated(18)
        verses.width = 260
        settled()
        verses.size = Type.lyricXLarge
        verses.height = 450
        settled()
        const row = verses.itemAtIndex(verses.current)
        row.translation = "A late translation of the active line that wraps over several rows and arrives without a new current index."
        settled()
    }
    function test_model_replacement_and_sync_revision_keep_follow_while_paused() {
        Player.paused = true
        verses.synced = false
        verses.lines = document().map((line, i) => ({text:line.text + " with a newly loaded verse",start_ms:line.start_ms}))
        verses.translation = translated(12)
        verses.synced = true
        settled()
        verify(verses.follow)
        verses.current = 13
        verses.translation = translated(20)
        settled()
        verify(verses.follow)
        Player.paused = false
    }
    function test_manual_follow_off_preserves_the_reader_position_through_translation() {
        verses.follow = false
        verses.contentY += 60
        const at = verses.contentY
        verses.translation = translated(20)
        verses.current = 14
        wait(350)
        fuzzyCompare(verses.contentY, at, 1, "polish may round the reader position by one device pixel")
        verses.follow = true
        settled()
    }
    function test_a_real_drag_turns_off_follow_but_programmatic_layout_does_not() {
        verses.translation = translated(14)
        settled()
        verify(verses.follow)
        mouseDrag(verses, verses.width / 2, verses.height / 2, 0, -100, Qt.LeftButton, Qt.NoModifier, 100)
        tryCompare(verses, "follow", false)
        verses.cancelFlick()
        const at = verses.contentY
        verses.current = 15
        wait(350)
        compare(verses.contentY, at)
        verses.follow = true
        settled()
    }
    function test_a_scrollbar_drag_leaves_the_reader_in_charge() {
        const bar = verses.ScrollBar.vertical
        mouseMove(verses, verses.width / 2, verses.height / 2)
        tryVerify(() => bar.visible)
        const thumb = bar.contentItem
        mousePress(bar, bar.width / 2, thumb.y + thumb.height / 2)
        mouseMove(bar, bar.width / 2, thumb.y + thumb.height / 2 + 60, 50)
        mouseRelease(bar, bar.width / 2, thumb.y + thumb.height / 2 + 60)
        tryCompare(verses, "follow", false)
    }
    function test_a_mouse_wheel_leaves_the_reader_in_charge() {
        mouseWheel(verses, verses.width / 2, verses.height / 2, 0, -120)
        tryCompare(verses, "follow", false)
    }
    function test_the_follow_overlay_dies_with_its_view() {
        const count = () => test.children.filter(child => child.objectName === "lyricsFollow").length
        const before = count()
        const view = temporary.createObject(test)
        wait(0)
        compare(count(), before + 1)
        view.destroy()
        wait(0)
        compare(count(), before)
    }
}
