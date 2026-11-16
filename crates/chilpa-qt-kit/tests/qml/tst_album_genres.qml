import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"

TestCase {
    name: "AlbumGenres"
    when: windowShown; visible: true; width: 500; height: 300
    AlbumGenreLine { id: line; width: parent.width }
    SignalSpy { id: more; target: line; signalName: "moreRequested" }
    function init() { failOnWarning(/ReferenceError|TypeError|Binding loop|invalid context/); line.document = ({}); Navigation.went = [] }
    function test_each_tag_keeps_its_text_and_routes_to_the_scoped_genre() {
        const key = JSON.stringify({kind:"genre",name:"Heavy Metal",excluded_sources:["plugin:other"]})
        line.document = {items:[{name:"heavy-metal",key:key},{name:"My <Sunday> & friends",key:JSON.stringify({kind:"genre",name:"My <Sunday> & friends"})}]}
        const tag = findChild(line, "albumGenre:heavy-metal")
        verify(tag); compare(tag.text, "heavy-metal")
        verify(findChild(line, "albumGenre:My <Sunday> & friends"))
        waitForRendering(line); tag.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(Navigation.went, ["genres",key])
    }
    function test_empty_genres_do_not_leave_a_row_and_long_tags_fit() {
        verify(!line.visible)
        line.width = 210
        const name = "Curated genre ".repeat(20)
        line.document = {items:[{name:name,key:"{}"}]}
        verify(line.visible)
        const tag = findChild(line, "albumGenre:" + name)
        verify(tag.width <= line.width)
        line.document = ({}); verify(!line.visible)
        line.width = Qt.binding(() => parent.width)
    }
    function test_many_genres_create_eight_chips_and_a_keyboard_reachable_browser_action() {
        more.clear()
        line.document = {items:Array.from({length:20000}, (_, index) => ({name:"Genre " + index,key:"{}"})),total:20000}
        verify(findChild(line, "albumGenre:Genre 7"))
        compare(findChild(line, "albumGenre:Genre 8"), null)
        const button = findChild(line, "albumGenresMore")
        verify(button.visible)
        waitForRendering(line); button.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(more.count, 1)
        compare(Navigation.went, [])
    }
    function test_failed_genres_remain_visible_and_open_the_retry_browser() {
        more.clear()
        line.document={items:[],total:0,hasMore:false,error:"The genre index could not be completed."}
        verify(line.visible)
        const button=findChild(line,"albumGenresMore")
        verify(button.visible)
        button.clicked();compare(more.count,1)
    }
}
