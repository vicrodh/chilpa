import QtQuick
import QtTest
import "../../qml/Primitives"

TestCase {
    name: "PillButton"
    when: windowShown
    visible: true
    width: 560; height: 240
    Component { id: maker; PillButton {} }
    FontMetrics { id: metrics }
    function label(item, value) {
        for (const child of item.children || []) {
            if (child.text === value && child.truncated !== undefined) return child
            const found = label(child, value)
            if (found) return found
        }
        return null
    }
    function test_long_metadata_is_elided_inside_the_maximum_width() {
        const button = createTemporaryObject(maker, this, {text:"A long recording title ".repeat(30),maximumWidth:220})
        const text = label(button, button.text)
        verify(text)
        tryVerify(() => text.truncated)
        verify(text.width <= button.width)
        compare(button.text, "A long recording title ".repeat(30))
    }
    function test_metadata_with_angle_brackets_remains_literal() {
        const button = createTemporaryObject(maker, this, {text:"<b>Genre</b>"})
        const text = label(button, button.text)
        metrics.font = text.font
        tryVerify(() => Math.round(text.contentWidth) === Math.round(metrics.advanceWidth(button.text)))
    }
    function glyph(item) {
        for (const child of item.children || []) {
            if (child.glyph !== undefined && child.size !== undefined) return child
            const found = glyph(child)
            if (found) return found
        }
        return null
    }
    // A step back carries its arrow before the words, a step on after them.
    function test_the_glyph_goes_after_the_words_only_when_asked() {
        const back = createTemporaryObject(maker, this, {text: "Back", glyph: ""})
        tryVerify(() => glyph(back).x < label(back, "Back").x)
        const next = createTemporaryObject(maker, this, {text: "Next", glyph: "", glyphAfter: true})
        tryVerify(() => glyph(next).x > label(next, "Next").x)
        compare(next.implicitWidth, back.implicitWidth + label(next, "Next").implicitWidth - label(back, "Back").implicitWidth)
    }
    function test_wrapped_copy_keeps_its_lines() {
        const button = createTemporaryObject(maker, this, {text:"A detailed explanation ".repeat(8),maximumWidth:150,wrapText:true})
        const text = label(button, button.text)
        tryVerify(() => text.lineCount > 1)
        verify(!text.truncated)
        verify(button.height > 40)
    }
}
