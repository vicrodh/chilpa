import QtQuick
import QtTest
import "../../qml/Theme"
import "../../qml/Components"

TestCase {
    name: "CardLayout"
    when: windowShown
    visible: true
    width: 1200; height: 800
    ShelfSkeleton { id: shelf; width: 1000 }
    function test_narrow_sections_keep_the_library_cover_size() {
        for (const viewport of [960, 1280, 1720]) {
            CardLayout.viewportWidth = viewport
            const reference = Metrics.albumCardWidth
            compare(CardLayout.cardWidth, 200, "resizing the window never resizes its covers")
            compare(shelf.cardWidth, reference)
            shelf.width = 650
            compare(shelf.cardWidth, reference, "a sidebar fits fewer cards instead of shrinking them")
            verify(CardLayout.columns(650) < CardLayout.columns(viewport))
        }
        CardLayout.viewportWidth = 0
    }
}
