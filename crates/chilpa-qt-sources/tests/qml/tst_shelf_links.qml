import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

TestCase {
    name: "ShelfLinks"
    when: windowShown
    visible: true
    width: 1000; height: 800
    SourceShelf { id: shelf; width: parent.width }
    function test_complete_sections_do_not_offer_another_page() {
        shelf.section = {title:"Recent",key:"fixture native:home|recent",items:[{ref:"album:1"}]}
        compare(findChild(shelf, "shelfHeader").link, "")
    }
    function test_only_a_resolved_continuation_offers_another_page() {
        shelf.section = {title:"Recent",key:"fixture native:home|recent",more:"browse:next"}
        compare(findChild(shelf, "shelfHeader").link, "")
        shelf.section = {title:"Recent",key:"fixture native:home|recent",more:"browse:next",more_key:"fixture native:browse:next"}
        const header = findChild(shelf, "shelfHeader")
        verify(header.link.length > 0)
        header.linkClicked()
        compare(Navigation.went, ["source", "fixture native:browse:next"])
        shelf.wrap = true
        compare(header.link, "")
        shelf.wrap = false
    }
    function test_a_tag_without_a_cursor_has_no_load_more_button() {
        shelf.wrap = true
        shelf.section = {title:"Releases",key:"fixture native:artist:1|albums",more:"artist:1:albums:10",error:"Unavailable",
            tags:[{slug:"live",name:"Live",key:"fixture native:artist:1|albums#live"}]}
        const more = findChild(shelf, "shelfLoadMore")
        verify(more.visible)
        shelf.selectedTag = "live"
        compare(more.visible, false)
        shelf.selectedTag = ""
        shelf.wrap = false
        shelf.section = {}
    }
}
