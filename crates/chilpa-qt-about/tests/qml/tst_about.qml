import QtQuick
import QtTest
import app.chilpa.kit
import "../../qml/Components"

TestCase {
    name: "About"
    when: windowShown
    visible: true
    width: 1100
    height: 900

    AboutModal { id: about }

    function cleanup() { about.close() }

    function test_it_says_the_name_the_version_and_the_build() {
        about.open()
        tryCompare(about, "opened", true)
        compare(findChild(about.contentItem, "aboutName").text, "Chilpa")
        compare(findChild(about.contentItem, "aboutVersion").text, "v1.2.3")
        verify(about.width <= 920 && about.height <= 860, "no larger than QBZ's")
        const facts = []
        const walk = (item) => {
            if (item.objectName === "aboutFact") facts.push(item.text)
            for (const child of item.children || []) walk(child)
        }
        walk(about.contentItem)
        compare(facts.sort(), ["1.2.3", "2026-10-09 (abcdef123)", "Linux (Qt)"].sort())
        const link = findChild(about.contentItem, "aboutLink")
        compare(link.text, "Website")
    }

    function test_escape_and_the_close_button_close_it() {
        about.open()
        tryCompare(about, "opened", true)
        keyClick(Qt.Key_Escape)
        tryCompare(about, "visible", false)
        about.open()
        tryCompare(about, "opened", true)
        mouseClick(findChild(about.contentItem, "aboutClose"))
        tryCompare(about, "visible", false)
    }
}
