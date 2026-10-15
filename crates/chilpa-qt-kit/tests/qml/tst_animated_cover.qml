import QtQuick
import QtTest
import "../../qml/Primitives"

TestCase {
    name: "AnimatedCover"
    when: windowShown
    visible: true
    width: 220; height: 220
    AnimatedCover {
        id: cover
        width: 180; height: 180
        colors: ["#221725", "#d17655", "#f8dda7"]
        label: "Daily mix"
    }
    function test_hidden_labels_clear_the_shaped_text_and_restore_the_full_label() {
        const text = cover.children.find(child => typeof child.text === "string")
        verify(text)
        compare(text.text, cover.label)
        cover.visible = false
        compare(text.text, "")
        cover.label = "A complete title with CJK 日本語 and accents áé"
        compare(text.text, "")
        cover.visible = true
        compare(text.text, cover.label)
    }
    function test_color_moves_only_while_visible() {
        const initial = cover.phase
        tryVerify(() => cover.phase > initial, 500)
        cover.visible = false
        const paused = cover.phase
        wait(120)
        compare(cover.phase, paused)
        cover.visible = true
    }
}
