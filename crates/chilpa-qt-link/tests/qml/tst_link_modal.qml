import QtQuick
import QtTest
import app.chilpa
import app.chilpa.link
import "../../qml/Components"

// The modal opens empty, sends what is pasted, says what went wrong, takes a
// playlist to the importer and opens where the link leads.
TestCase {
    id: test
    name: "LinkModal"
    when: windowShown
    visible: true
    width: 800
    height: 600

    LinkModal { id: modal }

    function cleanup() {
        Link.close()
        Link.error = ""
        Link.playlist = false
        Link.resolving = false
        tryCompare(modal, "visible", false)
    }

    function test_it_opens_empty_and_sends_what_is_pasted() {
        Link.show()
        tryCompare(modal, "visible", true)
        const field = findChild(modal.contentItem, "linkField")
        const go = findChild(modal.contentItem, "linkGo")
        compare(field.text, "")
        verify(!go.enabled, "nothing to send yet")
        field.text = "https://example.org/album/1"
        verify(go.enabled)
        mouseClick(go)
        compare(Link.submitted, "https://example.org/album/1")
        verify(!go.enabled, "one link at a time")
    }

    function test_it_says_what_went_wrong() {
        Link.show()
        tryCompare(modal, "visible", true)
        Link.error = "Not in your library or in your sources"
        verify(findChild(modal.contentItem, "linkError").visible)
    }

    function test_a_playlist_goes_to_the_importer() {
        Link.show()
        tryCompare(modal, "visible", true)
        verify(!findChild(modal.contentItem, "linkPlaylist").visible)
        Link.playlist = true
        // The column places the button it just showed on its next polish.
        waitForRendering(modal.contentItem)
        mouseClick(findChild(modal.contentItem, "linkImporter"))
        compare(Link.importers, 1)
        compare(Navigation.page, "playlistImport")
        tryCompare(modal, "visible", false)
    }

    function test_it_opens_where_the_link_leads() {
        Link.opened("album", 7, "")
        compare(Navigation.page, "album")
        compare(Navigation.selectedId, 7)
        Link.opened("source", 0, "plugin native:artist:9")
        compare(Navigation.went, ["source", "plugin native:artist:9"])
    }

    function test_closing_the_modal_forgets_the_link() {
        Link.show()
        tryCompare(modal, "visible", true)
        keyClick(Qt.Key_Escape)
        tryCompare(Link, "open", false)
    }
}
