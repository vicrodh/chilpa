import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../../../chilpa-qt-kit/qml/Theme/TextLinks.js" as TextLinks

TestCase {
    name: "ArtistLinks"; when: windowShown; visible: true; width: 700; height: 200
    function init() { ThemeModel.palette = {"text.onAccentMuted":"#d1ffffff"} }
    function cleanup() { ThemeModel.palette = {} }
    Rectangle { anchors.fill: parent; color: "#842020" }
    AlbumArtistLine {
        id: line; x: 20; y: 20; width: 660; color: "white"; contrasted: true
        document: ({artist_html:'Various Artists (<a href="0">First artist</a>, <a href="1">Second artist</a>)',artist_links:[{target:"artist",key:"First"},{target:"artist",key:"Second"}]})
    }
    function pointFor(link) {
        for (let y = 0; y < line.height; ++y)
            for (let x = 0; x < line.width; ++x)
                if (line.linkAt(x, y) === link) return Qt.point(x + 1, y + 1)
        fail("The link must have a hit area: " + link)
    }
    function test_hover_changes_only_its_anchor_and_preserves_navigation() {
        mouseMove(line, line.width - 1, line.height - 1)
        waitForRendering(line)
        const before = line.text
        const original = grabImage(line)
        const secondStart = pointFor("1").x
        const first = pointFor("0")
        mouseMove(line, first.x, first.y)
        tryCompare(line, "hoveredLink", "0")
        waitForRendering(line)
        const highlighted = grabImage(line)
        let changed = 0
        for (let y = 0; y < original.height; ++y) {
            for (let x = 0; x < original.width; ++x) {
                if (!Qt.colorEqual(original.pixel(x, y), highlighted.pixel(x, y))) {
                    verify(x < secondStart, "Other artists must keep their resting pixels")
                    ++changed
                }
            }
        }
        verify(changed > 0, "The hovered artist must visibly brighten")
        const secondTag = before.match(/<a href="1"[^>]*>/)[0]
        verify(line.text.includes(secondTag))
        verify(line.text !== before)
        verify(line.text.includes("text-decoration: none"))
        compare(line.activeColor, line.color)
        mouseClick(line, first.x, first.y)
        compare(Navigation.went, ["artist", "First"])
        const second = pointFor("1")
        mouseMove(line, second.x, second.y)
        tryCompare(line, "hoveredLink", "1")
        mouseClick(line, second.x, second.y)
        compare(Navigation.went, ["artist", "Second"])
    }
    function test_provider_anchor_styles_cannot_reintroduce_underlines() {
        const html = TextLinks.render('<a href="artist:1" style="text-decoration:underline;color:red">Artist</a>', "", "white", "white")
        verify(!html.includes("decoration:underline"))
        verify(html.includes("text-decoration: none"))
    }
}
