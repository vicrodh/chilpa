import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

TestCase {
    name: "AlbumInformation"; when: windowShown; visible: true; width: 1200; height: 820
    TextEdit { id: pasteTarget; visible: false }
    AlbumInfoModal { id: modal; albumId: 1; summary: "18 songs · 1 h" }
    function init() {
        SettingsModel.density = "rich"
        modal.albumId = 1
        AlbumActions.informationDocument = {title:"Record",artist:"Band",label:"Publisher",release_date:"2001",track_credits:[{title:"Song",artist:"Band",participants:[{name:"Player",roles:"Guitar",target:"artist",key:"Player"},{name:"Singer",roles:"Vocals",target:"artist",key:"Singer"}]}],description:"<p>Review text</p>"}
        modal.open()
        tryCompare(modal, "opened", true)
        waitForRendering(modal.contentItem)
    }
    function cleanup() { modal.close(); wait(0) }
    function test_bounded_modal_has_scroll_and_expandable_selectable_credits() {
        verify(modal.width < width)
        verify(modal.height < height)
        const reading = findChild(modal, "albumInformationScroll")
        verify(reading.height > 0 && reading.height < modal.height)
        const loader = findChild(modal, "albumCreditsLoader")
        const toggle = findChild(loader.item, "expandCredit")
        verify(toggle)
        mouseClick(toggle)
        const text = findChild(loader.item, "creditText")
        verify(text.visible)
        verify(text.readOnly && text.selectByMouse)
        compare(text.text, "Guitar")
        const richHeight = toggle.parent.height
        SettingsModel.density = "compact"
        waitForRendering(modal.contentItem)
        verify(toggle.parent.height < richHeight)
        compare(text.text, "Guitar")
        const tabs = findChild(modal, "albumInformationTabs")
        tabs.activated(1)
        const review = findChild(modal, "albumReview")
        verify(review.visible)
        verify(review.readOnly && review.selectByMouse)
        compare(review.getText(0, review.length), "Review text")
    }
    function test_artist_text_keeps_the_selected_route_after_close() {
        AlbumActions.informationDocument = {title:"Compilation",artist_html:'Various artists (<a href="0">First</a>, <a href="1">Second</a>)',artist_links:[{target:"source",key:"fixture native:artist:1"},{target:"source",key:"fixture native:artist:2"}]}
        const line = findChild(modal, "albumArtistLine")
        verify(line.text.startsWith("Various artists ("))
        line.linkActivated("1")
        compare(Navigation.went, ["source", "fixture native:artist:2"])
        verify(!modal.visible)
    }
    function test_participant_navigation_closes_the_modal() {
        const loader = findChild(modal, "albumCreditsLoader")
        mouseClick(findChild(loader.item, "expandCredit"))
        waitForRendering(modal.contentItem)
        mouseClick(findChild(loader.item, "creditParticipant"))
        compare(Navigation.went, ["artist", "Player"])
        verify(!modal.visible)
    }
    function test_copy_actions_use_the_selected_tab_and_plain_review() {
        AlbumActions.informationDocument = {title:"Record",copy_basic:"Record — Band",copy_header:"Record — Band\nPublisher",copy_credits:"Record — Band\nPlayer — Guitar",description:"<p>Review text</p>"}
        const menu = findChild(modal, "albumCopyMenu")
        const clipboard = findChild(modal, "albumClipboard")
        menu.chosen(0)
        compare(clipboard.text, "Record — Band")
        menu.chosen(1)
        compare(clipboard.text, "Record — Band\nPlayer — Guitar")
        modal.activeTab = 1
        menu.chosen(1)
        compare(clipboard.text, "Record — Band\nPublisher\n\nReview text")
        const external = findChild(modal, "albumExternalMenu")
        external.url = "https://example.test/record"
        external.chosen(1)
        compare(clipboard.text, external.url)
        pasteTarget.text = ""
        pasteTarget.paste()
        compare(pasteTarget.text, external.url)
    }
    function test_sparse_local_metadata_still_opens_and_closes_with_escape() {
        AlbumActions.informationDocument = {title:"Local record",artist:"Band",release_date:"1999"}
        verify(modal.visible)
        keyClick(Qt.Key_Escape)
        tryCompare(modal, "visible", false)
    }
    function test_close_button_and_album_switch_dismiss_the_modal() {
        mouseClick(findChild(modal, "closeAlbumInformation"))
        tryCompare(modal, "visible", false)
        modal.open()
        modal.albumId = 2
        verify(!modal.visible)
    }
    function test_label_navigation_keeps_the_source_after_close() {
        AlbumActions.informationDocument = {title:"Record",plugin:"fixture",label:"Publisher",label_ref:"label:9"}
        waitForRendering(modal.contentItem)
        mouseClick(findChild(modal, "albumLabel"))
        compare(Navigation.went, ["source","fixture native:label:9"])
        verify(!modal.visible)
    }
}
