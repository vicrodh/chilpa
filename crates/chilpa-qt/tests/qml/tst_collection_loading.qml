import QtQuick
import QtTest
import app.chilpa
import "../../qml/Screens"

TestCase {
    name:"CollectionLoading"
    when:windowShown
    visible:true
    width:1100; height:820
    Component { id:artists; ArtistsScreen { width:1100; height:820 } }
    Component { id:albums; AlbumsScreen { width:1100; height:820; source:"" } }
    function init() { failOnWarning(/ReferenceError|TypeError|Binding loop|invalid context/) }
    function test_pending_and_failed_collections_do_not_present_zero_results_data() {
        return [{tag:"artists",maker:artists,prefix:"artistsPage"},{tag:"albums",maker:albums,prefix:"albumsPage"}]
    }
    function test_pending_and_failed_collections_do_not_present_zero_results(data) {
        const page=createTemporaryObject(data.maker,this)
        verify(page)
        const model=findChild(page,data.prefix+"Model")
        const heading=findChild(page,data.prefix+"Heading")
        const skeleton=findChild(page,data.prefix+"Loading")
        const grid=findChild(page,data.prefix+"Grid")
        const message=findChild(page,data.prefix+"Message")
        model.albums=0;model.ready=false
        compare(heading.count,"")
        verify(skeleton.visible);verify(!grid.visible);verify(!message.visible)
        model.error="reader temporarily unavailable";model.ready=true
        compare(heading.count,"")
        verify(!skeleton.visible);verify(!grid.visible);verify(message.visible)
        findChild(page,data.prefix+"Retry").clicked()
        compare(model.retries,1)
        verify(skeleton.visible);verify(!message.visible)
        model.albums=4;model.ready=true
        verify(heading.count!=="")
        verify(grid.visible);verify(!skeleton.visible);verify(!message.visible)
        model.ready=false
        verify(skeleton.visible);verify(!grid.visible)
        model.ready=true
        verify(grid.visible)
    }
}
