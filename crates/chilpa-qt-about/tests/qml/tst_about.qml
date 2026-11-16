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
        verify(about.width <= 920 && about.height <= 860, "within the modal size limit")
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

    function test_closed_modals_release_and_recreate_their_body() {
        about.close()
        tryCompare(about, "visible", false)
        const loader = findChild(about.contentItem, "aboutBodyLoader")
        compare(loader.item, null)
        for (let n = 0; n < 3; ++n) {
            about.open()
            tryCompare(about, "opened", true)
            verify(loader.item !== null)
            compare(findChild(loader.item, "aboutAuthor").handle, "vicrodh")
            const flow = findChild(loader.item, "aboutContributorFlow")
            compare(flow.children.filter(item => item.objectName === "aboutContributor").length, 3)
            about.close()
            tryCompare(about, "visible", false)
            compare(loader.item, null)
        }
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
