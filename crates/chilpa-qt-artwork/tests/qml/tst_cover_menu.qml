import QtQuick
import QtTest
import app.chilpa.kit
import "../../qml/Components"

TestCase {
    name: "CoverMenu"
    when: windowShown
    visible: true
    width: 900
    height: 700

    CoverMenu { id: menu }
    CoverLightbox { id: lightbox }
    SignalSpy { id: chose; target: menu; signalName: "choose" }

    function texts() { return menu.entries.map(entry => entry.text) }
    function cleanup() {
        menu.close()
        lightbox.close()
        menu.kind = "album"
    }

    function test_a_found_cover_offers_to_add_one_view_it_and_save_it() {
        menu.custom = false; menu.showing = true; menu.address = ""
        menu.build()
        compare(texts(), ["Add cover", "View cover", "Save as…"])
    }

    function test_a_chosen_cover_can_change_or_go_and_the_web_shows_when_there_is_an_address() {
        menu.custom = true; menu.showing = true; menu.address = "https://example.org/cover.jpg"
        menu.build()
        compare(texts(), ["Change cover", "Remove cover", "View cover", "Open in browser", "Save as…"])
    }

    function test_an_artist_without_a_picture_can_only_be_given_one() {
        menu.kind = "artist"; menu.custom = false; menu.showing = false; menu.address = ""
        menu.build()
        compare(texts(), ["Add image"])
    }

    function test_a_row_says_what_was_chosen() {
        menu.custom = false; menu.showing = false; menu.address = ""
        chose.clear()
        menu.popupAt(10, 10)
        tryCompare(menu, "opened", true)
        menu.chosen(0)
        compare(chose.count, 1)
    }

    function test_the_lightbox_closes_on_escape_and_on_the_veil_but_not_on_the_picture() {
        lightbox.show(Qt.resolvedUrl("../../../chilpa-qt-kit/assets/logo.png"))
        tryCompare(lightbox, "opened", true)
        const picture = findChild(lightbox.contentItem, "lightboxPicture")
        tryCompare(picture, "status", Image.Ready)
        verify(picture.width <= lightbox.width * 0.9 + 1 && picture.height <= lightbox.height * 0.9 + 1, "inside nine tenths of the window")
        mouseClick(picture, picture.width / 2, picture.height / 2)
        verify(lightbox.visible, "a click on the picture keeps it open")
        keyClick(Qt.Key_Escape)
        tryCompare(lightbox, "visible", false)
        tryCompare(picture, "status", Image.Null)
        compare(lightbox.source.toString(), "", "closing releases the display-sized image")
        lightbox.show(Qt.resolvedUrl("../../../chilpa-qt-kit/assets/logo.png"))
        tryCompare(lightbox, "opened", true)
        tryCompare(picture, "status", Image.Ready, 1000, "the same cover can be reopened")
        mouseClick(findChild(lightbox.contentItem, "lightboxVeil"), 4, 4)
        tryCompare(lightbox, "visible", false)
    }
}
