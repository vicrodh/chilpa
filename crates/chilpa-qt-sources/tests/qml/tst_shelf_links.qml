import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "../../qml/Patterns"

TestCase {
    name: "ShelfLinks"
    when: windowShown
    visible: true
    width: 1000; height: 800
    Flickable {
        id: viewport
        width: parent.width; height: parent.height
        contentWidth: width; contentHeight: shelf.height + height
        clip: true
        SourceShelf { id: shelf; width: parent.width; viewport: viewport }
    }
    function visibleArtist(item) {
        if (item.objectName === "albumCardArtist" && item.visible) return item
        for (const child of item.children || []) {
            const found = visibleArtist(child)
            if (found) return found
        }
        return null
    }
    function test_home_album_artist_is_clickable_inside_the_horizontal_rail() {
        Sources.pages = {"fixture native:home":JSON.stringify({shelves:[{id:"albums",items:[{ref:"album:1"}]}]})}
        AlbumActions.artistDestination = {target:"source",key:"fixture native:artist:17"}
        Navigation.went = []
        shelf.oneRow = true
        shelf.section = {id:"albums",title:"Records",key:"fixture native:home|albums"}
        tryVerify(() => visibleArtist(shelf) !== null)
        const artist = visibleArtist(shelf)
        waitForRendering(artist)
        tryVerify(() => artist.visible && artist.enabled, 1000, "The album artist must accept pointer input")
        const rail = findChild(shelf, "sourceShelfRail")
        verify(artist.mapToItem(rail, 0, artist.height).y <= rail.height,
            "The artist credit must fit inside the clipped album rail")
        for (let ancestor = artist.parent; ancestor; ancestor = ancestor.parent) {
            if (!ancestor.clip) continue
            const point = artist.mapToItem(ancestor, artist.width / 2, artist.height / 2)
            verify(point.x >= 0 && point.x < ancestor.width && point.y >= 0 && point.y < ancestor.height,
                "The artist credit must stay reachable inside " + ancestor)
        }
        mouseClick(artist)
        compare(Navigation.went, ["source", "fixture native:artist:17"])
        AlbumActions.artistDestination = {}
        shelf.oneRow = false
        shelf.section = {}
    }
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
