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
        compare(copy.width, made.cut.width)
        compare(copy.mapToItem(test, 0, 0).x, made.cut.mapToItem(test, 0, 0).x)
        compare(words.text, test.longTitle)
        verify(words.over > 0)
        tryVerify(() => words.x < 0, Motion.tickerRest * 3, "after its rest the copy moves toward the end")
        tryVerify(() => Math.abs(words.x + words.over) < 1, Motion.tickerRest + words.over * Motion.tickerPerPixel * 2, "and stops with its last letter in view")
        TipState.hide(made.cut)
        tryVerify(() => slide() === null)
        compare(made.cut.opacity, 1, "let go, the line is drawn again as it was")
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
        TipState.hide(made.cut)
        SettingsModel.preferences = {}
        Prefs.rev++
        Motion.reduced = true
        TipState.show(made.cut.text, made.cut)
        wait(Motion.fast || 100)
        compare(slide(), null)
    }
}
