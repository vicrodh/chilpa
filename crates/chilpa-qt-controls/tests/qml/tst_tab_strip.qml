import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/qml/Patterns"

TestCase {
    name: "TabStripLifecycle"
    when: windowShown
    visible: true
    width: 900; height: 100
    TabStrip { id: strip; room: 900 }

    function tabs(item) {
        let found = []
        if (item.renamable !== undefined && item.sourceIcon !== undefined)
            found.push(item)
        for (const child of item.children || [])
            found = found.concat(tabs(child))
        return found
    }
    function init() {
        Navigation.tabTitles = ["First", "Second"]
        Navigation.tabIcons = ["", ""]
        Navigation.active = 0
        Navigation.closedIndex = -1
    }
    function test_same_count_updates_keep_the_mounted_tabs() {
        tryVerify(() => tabs(strip).length === 2)
        const before = tabs(strip)
        Navigation.tabTitles = ["Renamed", "Destination"]
        Navigation.tabIcons = ["", "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"]
        Navigation.active = 1
        const after = tabs(strip)
        compare(after.length, 2)
        compare(after[0], before[0], "I retain the mounted tab through a title publication")
        compare(after[1], before[1])
        compare(after[0].title, "Renamed")
        compare(after[1].title, "Destination")
        compare(after[1].sourceIcon, "qrc:/qt/qml/app/chilpa/kit/assets/logo.png")
        compare(after[0].active, false)
        compare(after[1].active, true)
    }
    function test_growing_and_closing_keep_current_callback_indices() {
        Navigation.tabTitles = ["First", "Second", "Third"]
        Navigation.tabIcons = ["", "", ""]
        tryVerify(() => tabs(strip).length === 3)
        tabs(strip)[2].activated()
        compare(Navigation.active, 2)
        tabs(strip)[1].closed()
        compare(Navigation.closedIndex, 1)
        Navigation.tabTitles = ["First", "Third"]
        Navigation.tabIcons = ["", ""]
        tryVerify(() => tabs(strip).length === 2)
        compare(tabs(strip)[1].title, "Third")
        tabs(strip)[1].activated()
        compare(Navigation.active, 1)
        tabs(strip)[1].closed()
        compare(Navigation.closedIndex, 1)
    }
}
