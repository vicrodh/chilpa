import QtQuick
import QtTest
import com.blitzfc.chilpa
import "../../qml/Patterns"

TestCase {
    name: "AlbumInformation"; when: windowShown; visible: true; width: 1200; height: 820
    AlbumInfoModal { id: modal; albumId: 1; summary: "18 songs · 1 h" }
    function init() {
        SettingsModel.density = "rich"
        modal.albumId = 1
        AlbumActions.informationDocument = {title:"Record",artist:"Band",label:"Publisher",release_date:"2001",track_credits:[{title:"Song",artist:"Band",performers:"Player, Guitar\nSinger, Vocals"}],description:"<p>Review text</p>"}
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
        compare(text.text, "Player, Guitar\nSinger, Vocals")
        const richHeight = toggle.parent.height
        SettingsModel.density = "compact"
        waitForRendering(modal.contentItem)
        verify(toggle.parent.height < richHeight)
        verify(text.text.startsWith("Band"))
        const tabs = findChild(modal, "albumInformationTabs")
        tabs.activated(1)
        const review = findChild(modal, "albumReview")
        verify(review.visible)
        verify(review.readOnly && review.selectByMouse)
        compare(review.getText(0, review.length), "Review text")
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
        AlbumActions.informationDocument = {title:"Record",plugin:"fixture",label_ref:"label:9"}
        modal.follow("label:9")
        compare(Navigation.went, ["source","fixture native:label:9"])
        verify(!modal.visible)
    }
}
