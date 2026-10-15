import QtQuick
import QtTest
import "../../qml/Primitives"
import "../../qml/Theme"

TestCase {
    id: test
    name: "IconGlyphFamily"
    when: windowShown
    visible: true
    width: 200; height: 200
    property bool observing: false
    property var mismatches: []
    property int emptyFrames: 0
    Icon { id: icon; glyph: Icons.heart }
    FontLoader { id: lateFont }
    Connections {
        target: icon
        function onTextChanged() { test.checkPair() }
        function onFontChanged() { test.checkPair() }
    }
    FrameAnimation {
        running: test.observing
        onTriggered: if (icon.glyph !== "" && icon.text === "") test.emptyFrames++
    }
    function checkPair() {
        if (!observing || icon.text === "") return
        const family = icon.text === Icons.heart ? Icons.outline : Icons.filled
        if (icon.font.family !== family) mismatches.push({glyph:icon.text.charCodeAt(0),family:icon.font.family,expected:family})
    }
    function settled(glyph, family) {
        tryCompare(icon, "text", glyph)
        tryCompare(icon.font, "family", family)
        compare(mismatches.length, 0, JSON.stringify(mismatches))
        waitForRendering(icon)
        compare(emptyFrames, 0, "I never paint an empty transition frame")
    }
    function initTestCase() {
        compare(Icons.outlineFont.status, FontLoader.Ready)
        compare(Icons.filledFont.status, FontLoader.Ready)
        verify(Icons.outline !== "" && Icons.filled !== "" && Icons.outline !== Icons.filled)
    }
    function init() {
        observing = false
        lateFont.source = "../../assets/icons/tabler-outline.ttf"
        icon.iconFamily = ""; icon.filled = false; icon.glyph = Icons.heart
        tryCompare(icon, "text", Icons.heart)
        tryCompare(icon.font, "family", Icons.outline)
        mismatches = []; emptyFrames = 0; observing = true
    }
    function cleanup() { observing = false }
    function test_glyph_then_family_and_the_reverse_never_shape_a_mismatched_pair() {
        icon.glyph = Icons.heartFilled
        icon.filled = true
        settled(Icons.heartFilled, Icons.filled)
        icon.filled = false
        icon.glyph = Icons.heart
        settled(Icons.heart, Icons.outline)
        icon.filled = true
        icon.glyph = Icons.heartFilled
        settled(Icons.heartFilled, Icons.filled)
        icon.glyph = Icons.heart
        icon.filled = false
        settled(Icons.heart, Icons.outline)
    }
    function test_an_explicit_family_overrides_filled_and_keeps_text_properties() {
        icon.size = 32; icon.color = "orange"; icon.font.weight = Font.Bold
        icon.filled = true
        icon.iconFamily = Icons.outline
        settled(Icons.heart, Icons.outline)
        compare(icon.width, 32); compare(icon.height, 32)
        compare(icon.font.pixelSize, 32); compare(icon.font.weight, Font.Bold)
        compare(icon.color, "#ffa500")
    }
    function test_a_font_loader_name_change_uses_the_same_coherent_update() {
        icon.iconFamily = Qt.binding(() => lateFont.name)
        icon.glyph = Icons.heartFilled
        lateFont.source = "../../assets/icons/tabler-filled.ttf"
        tryCompare(lateFont, "status", FontLoader.Ready)
        settled(Icons.heartFilled, Icons.filled)
        icon.glyph = Icons.heart
        lateFont.source = "../../assets/icons/tabler-outline.ttf"
        tryCompare(lateFont, "status", FontLoader.Ready)
        settled(Icons.heart, Icons.outline)
    }
}
