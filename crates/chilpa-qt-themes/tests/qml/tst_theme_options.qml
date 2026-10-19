import QtQuick
import QtTest
import app.chilpa.kit
import app.chilpa.themes
import "../../qml/Patterns"

// The theme's picker filters its list as QBZ's does, and the custom
// theme's options send what is edited: a colour while it is dragged, the
// write when the drag ends, the polarity and the seed.
TestCase {
    id: test
    name: "ThemeOptions"
    when: windowShown
    visible: true
    width: 900
    height: 900

    ThemePicker {
        id: picker
        options: ["Dark", "Nord", "Alucard", "Light", "Auto (dynamic)", "Custom"]
        values: ["dark", "nord", "alucard", "light", "auto", "custom"]
        value: "nord"
    }
    ThemeOptionsPanel { id: panel; y: 60; width: 800 }

    function init() {
        ThemeEditor.calls = []
        ThemeEditor.doc = Object.assign({}, ThemeEditor.doc, { theme: "custom", filter: 0 })
    }

    function test_the_filter_cycles_and_keeps_only_its_themes() {
        const list = findChild(picker, "themeList")
        compare(list.values.length, 6, "every theme, with auto and custom")
        mouseClick(findChild(picker, "themeFilter"))
        compare(ThemeEditor.calls[0], "filter 1")
        compare(list.values, ["dark", "nord"], "the dark ones, without auto and custom")
        mouseClick(findChild(picker, "themeFilter"))
        compare(list.values, ["alucard", "light"], "the light ones")
    }

    function test_a_swatch_opens_the_picker_and_a_drag_edits_its_colour() {
        const custom = findChild(panel, "customOptions")
        verify(custom.visible)
        const pickerItem = findChild(panel, "colorPicker")
        verify(!pickerItem.visible, "closed until a swatch is opened")
        const swatches = []
        function collect(item) {
            if (item.objectName === "colorSwatch") swatches.push(item)
            for (let i = 0; i < item.children.length; i++) collect(item.children[i])
        }
        collect(custom)
        compare(swatches.length, 11)
        mouseClick(swatches[5])
        tryCompare(pickerItem, "visible", true)
        const shade = findChild(pickerItem, "pickerShade")
        // A press right after the swatch's click would be a double click.
        wait(Qt.styleHints.mouseDoubleClickInterval)
        mousePress(shade, shade.width - 2, 2)
        mouseRelease(shade, shade.width - 2, 2)
        verify(ThemeEditor.calls.some((c) => c.startsWith("color accent #")), JSON.stringify(ThemeEditor.calls))
        compare(ThemeEditor.calls[ThemeEditor.calls.length - 1], "flush", "the release writes it")
        mouseClick(swatches[5])
        tryCompare(pickerItem, "visible", false)
    }

    function test_the_polarity_and_the_seed_are_sent() {
        mouseClick(findChild(panel, "customDark"))
        compare(ThemeEditor.calls[0], "dark false")
        mouseClick(findChild(panel, "customSeed"))
        compare(ThemeEditor.calls[1], "seed")
    }

    function test_the_auto_box_offers_the_image_only_for_the_image_source() {
        ThemeEditor.doc = Object.assign({}, ThemeEditor.doc, { theme: "auto" })
        verify(findChild(panel, "autoOptions").visible)
        verify(!findChild(panel, "customOptions").visible)
        verify(!findChild(panel, "autoImage").visible)
        ThemeEditor.doc = Object.assign({}, ThemeEditor.doc, { auto: Object.assign({}, ThemeEditor.doc.auto, { source: 2 }) })
        tryCompare(findChild(panel, "autoImage"), "visible", true)
        mouseClick(findChild(panel, "autoRegenerate"))
        compare(ThemeEditor.calls[ThemeEditor.calls.length - 1], "regenerate")
    }
}
