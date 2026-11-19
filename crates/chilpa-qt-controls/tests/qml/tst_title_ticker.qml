import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components"

// A line cut short slides to show its end while the window's tooltip is
// about it, and only then: a line that fits, the preference turned off or
// reduced motion leave every line as it is drawn.
TestCase {
    id: test
    name: "TitleTicker"
    when: windowShown
    visible: true
    width: 600
    height: 300

    readonly property string longTitle: "A title that is far too long for the place a row gives it"

    Component {
        id: factory
        Item {
            anchors.fill: parent
            property alias cut: cut
            property alias whole: whole
            property alias badged: badged
            Text { id: cut; x: 20; y: 20; width: 120; elide: Text.ElideRight; text: test.longTitle }
            Text { id: whole; x: 20; y: 60; width: 400; elide: Text.ElideRight; text: "Short" }
            AdvisoryTitle { id: badged; x: 20; y: 100; width: 140; explicitContent: true; text: test.longTitle }
            TitleTicker {}
        }
    }

    // As in the window: one watcher for every cut line, and the ticker sliding the one it says.
    Component {
        id: watched
        Item {
            anchors.fill: parent
            property alias first: first
            property alias second: second
            Text { id: first; x: 20; y: 20; width: 120; elide: Text.ElideRight; text: test.longTitle }
            Text { id: second; x: 20; y: 20; width: 120; elide: Text.ElideRight; visible: false; text: "Another title much too long for its place in a row" }
            CutTextTips { watching: parent }
            TitleTicker {}
        }
    }
    Component { id: lineFactory; Text { x: 20; y: 20; width: 120; elide: Text.ElideRight } }
    function watchedScene() {
        const made = createTemporaryObject(watched, test)
        verify(made)
        tryVerify(() => made.first.truncated)
        mouseMove(test, 40, 26)
        mouseMove(test, 42, 27)
        tryCompare(TipState, "target", made.first, Metrics.cutTextLook * 10, "the cut line says itself")
        return made
    }

    function slide() {
        const layer = test.Overlay.overlay
        for (const child of layer.children)
            if (child.objectName === "titleTickerSlide")
                return child
        return null
    }
    function scene() {
        const made = createTemporaryObject(factory, test)
        verify(made)
        tryVerify(() => made.cut.truncated && made.badged.truncated && !made.whole.truncated)
        return made
    }

    function init() {
        SettingsModel.preferences = {}
        Prefs.rev++
        Motion.reduced = false
    }
    function cleanup() {
        TipState.target = null
        TipState.shown = false
        Motion.reduced = false
    }

    function test_a_cut_line_slides_to_its_end_and_comes_back_when_let_go() {
        const made = scene()
        compare(slide(), null)
        TipState.show(made.cut.text, made.cut)
        tryVerify(() => slide() !== null)
        const copy = slide()
        const words = findChild(copy, "titleTickerWords")
        compare(made.cut.opacity, 0, "the line hides under its sliding copy")
        verify(TipState.quiet, "the tooltip stays out while the line shows itself by sliding")
        compare(copy.width, made.cut.width)
        compare(copy.mapToItem(test, 0, 0).x, made.cut.mapToItem(test, 0, 0).x)
        compare(words.text, test.longTitle)
        verify(words.over > 0)
        tryVerify(() => words.x < 0, Motion.tickerRest * 3, "after its rest the copy moves toward the end")
        tryVerify(() => Math.abs(words.x + words.over) < 1, Motion.tickerRest + words.over * Motion.tickerPerPixel * 2, "and stops with its last letter in view")
        TipState.hide(made.cut)
        tryVerify(() => slide() === null)
        compare(made.cut.opacity, 1, "let go, the line is drawn again as it was")
        verify(!TipState.quiet)
    }

    // TipState lets go of a line that hides; the watcher lets go of it too, and looks again under the still pointer.
    function test_a_cut_line_hidden_and_shown_again_under_a_still_pointer_says_itself_again() {
        const made = watchedScene()
        tryVerify(() => slide() !== null, 1000, "it slides")
        made.first.visible = false
        compare(TipState.target, null, "a hidden line says nothing")
        tryVerify(() => slide() === null, 1000, "and stops sliding")
        wait(Metrics.cutTextLook * 2)
        made.first.visible = true
        tryCompare(TipState, "target", made.first, Metrics.cutTextLook * 10, "shown again, it says itself again without the pointer moving")
        compare(TipState.text, test.longTitle)
        tryVerify(() => slide() !== null, 1000, "and slides again")
    }

    function test_a_cut_line_put_in_the_place_of_another_says_itself() {
        const made = watchedScene()
        made.first.visible = false
        made.second.visible = true
        tryCompare(TipState, "target", made.second, Metrics.cutTextLook * 10, "the line now under the still pointer says itself")
        compare(TipState.text, made.second.text)
        // One line goes for good and another is made in its place.
        const gone = createTemporaryObject(lineFactory, made, { text: test.longTitle + " (gone)" })
        made.second.visible = false
        tryCompare(TipState, "target", gone, Metrics.cutTextLook * 10)
        const next = createTemporaryObject(lineFactory, made, { text: test.longTitle + " (next)" })
        gone.destroy()
        tryCompare(TipState, "target", next, Metrics.cutTextLook * 10, "the line made in the place of a destroyed one says itself")
        compare(TipState.text, test.longTitle + " (next)")
    }

    // A menu opened from the keyboard, away from the still pointer, hides the tooltip on purpose: the line under the
    // pointer does not take it back while the menu is open, and the menu's own cut entry still says itself.
    Component { id: menuFactory; Menu { x: 260; y: 150; entries: [{ text: "Play" }, { text: test.longTitle + ", an entry of the menu" }] } }
    function test_a_menu_opened_under_a_still_pointer_keeps_the_line_quiet() {
        const made = watchedScene()
        tryVerify(() => slide() !== null, 1000, "it slides")
        const menu = createTemporaryObject(menuFactory, test)
        verify(menu.entries !== undefined, "the kit's menu")
        menu.open()
        tryVerify(() => menu.opened)
        compare(TipState.target, null, "the menu took the tooltip")
        tryVerify(() => slide() === null, 1000, "and the slide")
        wait(Metrics.cutTextLook * 4)
        compare(TipState.target, null, "the line under the still pointer does not take it back")
        compare(slide(), null, "nor slides again")
        const entry = menu.entryItems[1]
        verify(entry && entry.cut, "the long entry is cut")
        const at = entry.mapToItem(test, entry.width / 2, entry.height / 2)
        mouseMove(test, at.x, at.y)
        tryCompare(TipState, "target", entry, 1000, "the menu's cut entry says itself")
        wait(Metrics.cutTextLook * 4)
        compare(TipState.target, entry, "and keeps saying it")
        menu.close()
    }

    function test_a_line_that_fits_is_left_alone() {
        const made = scene()
        TipState.show(made.whole.text, made.whole)
        wait(Motion.fast)
        compare(slide(), null)
        compare(made.whole.opacity, 1)
    }

    function test_a_title_with_a_badge_slides_its_words_and_keeps_the_badge() {
        const made = scene()
        TipState.show(made.badged.text, made.badged)
        tryVerify(() => slide() !== null)
        compare(made.badged.opacity, 1, "what holds the badge stays")
        const words = findChild(made.badged, "advisoryText")
        compare(words.opacity, 0)
        verify(slide().width <= made.badged.width - made.badged.advisoryWidth, "the copy stays clear of the badge")
        TipState.hide(made.badged)
        tryVerify(() => words.opacity === 1)
    }

    function test_turned_off_or_with_reduced_motion_nothing_slides_and_the_tooltip_is_still_asked() {
        const made = scene()
        SettingsModel.preferences = {"appearance.titleTicker": "false"}
        Prefs.rev++
        TipState.show(made.cut.text, made.cut)
        wait(Motion.fast)
        compare(slide(), null)
        compare(made.cut.opacity, 1)
        compare(TipState.target, made.cut, "the tooltip is not the ticker's to turn off")
        verify(!TipState.quiet, "with nothing sliding the tooltip says the line")
        TipState.hide(made.cut)
        SettingsModel.preferences = {}
        Prefs.rev++
        Motion.reduced = true
        TipState.show(made.cut.text, made.cut)
        wait(Motion.fast || 100)
        compare(slide(), null)
    }
}
