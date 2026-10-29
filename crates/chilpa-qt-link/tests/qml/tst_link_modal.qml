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
        Link.choices = "[]"
        Link.chosen = -1
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

    function test_verified_destinations_can_be_chosen() {
        Link.show()
        tryCompare(modal, "visible", true)
        Link.choices = JSON.stringify([{ source: "Local", title: "Song", artist: "Band" },
                                      { source: "Another source", title: "Song", artist: "Band" }])
        const choices = findChild(modal.contentItem, "linkChoices")
        tryCompare(choices, "visible", true)
        // The list sizes its scroll area to the rows on its next polish.
        waitForRendering(modal.contentItem)
        const first = findChild(modal.contentItem, "linkChoice")
        verify(first)
        mouseClick(first)
        compare(Link.chosen, 0)
    }

    function test_choices_support_keyboard_navigation_and_accessible_labels() {
        Link.show()
        tryCompare(modal, "visible", true)
        Link.choices = JSON.stringify([{source:"Library", title:"Record", artist:"Band",local:true},
                                      {source:"Connected source", title:"Record", artist:"Band"}])
        const first = findChild(modal.contentItem, "linkChoice")
        tryCompare(first, "activeFocus", true)
        compare(first.Accessible.name, "Library, Record, Band")
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        compare(Link.chosen, 1)
    }

    function test_progress_is_visible_only_while_searching() {
        Link.show()
        tryCompare(modal, "visible", true)
        const progress = findChild(modal.contentItem, "linkProgress")
        verify(!progress.visible)
        Link.resolving = true
        verify(progress.visible)
        Link.resolving = false
        verify(!progress.visible)
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
