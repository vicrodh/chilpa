import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/query/qml/Patterns"

TestCase {
    name: "LibraryFilterLayout"
    when: windowShown
    visible: true
    width: 1000; height: 300
    LibraryPanes { id: panes; width: 1000; height: 300 }
    function test_a_single_filter_uses_the_full_width_data() {
        return ["year", "genre", "label", "composer", "rating", "bits", "rate", "artist", "album"].map(key => ({tag:key,column:key}))
    }
    function test_a_single_filter_uses_the_full_width(data) {
        failOnWarning(/ReferenceError|TypeError|invalid context/)
        panes.shown = [data.column]
        const widgets = () => Array.from(panes.children).reduce((items, child) => items.concat(Array.from(child.children || [])), [])
            .filter(child => child.column === data.column)
        tryVerify(() => widgets().length === 1)
        fuzzyCompare(widgets()[0].width, panes.width - 2, 0.1)
    }
}
