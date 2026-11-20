import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"
import "../../qml/Screens"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    id: test
    name: "AlbumInteraction"
    when: windowShown
    visible: true
    width: 1200; height: 800
    property int footerClicks: 0
    Component { id: pageMaker; AlbumScreen { width: 1100; height: 650 } }
    Component {
        id: listMaker
        TrackList {
            width: 1000; height: 600; rich: false; introHeight: 120
            model: TrackListModel {}
            footer: Flickable {
                objectName: "footerRail"
                width: 1000; height: 180; contentWidth: 2400; contentHeight: height
                flickableDirection: Flickable.HorizontalFlick
                MouseArea {
                    objectName: "footerCard"; width: 200; height: 160
                    onClicked: test.footerClicks++
                }
            }
        }
    }
    function fill(model, count) {
        for (let i = 1; i <= count; ++i)
            model.append({trackId:i,rowKind:0,number:i,title:"Song " + i,artist:"Band",album:"Album",
                sourceName:"",duration:"3:00",plays:0,quality:"FLAC",tier:2,available:true,playing:false,
                selected:false,headerLabel:"",headerMeta:"",art:""})
    }
    function test_scrollbar_uses_the_page_margin_after_resize_and_scroll() {
        const page=createTemporaryObject(pageMaker,this)
        fill(findChild(page,"albumTracks"),100)
        const bar=findChild(page,"trackListScrollbar")
        tryCompare(bar,"parent",page)
        for (const width of [1100,800]) {
            page.width=width
            page.scrollView.positionViewAtEnd()
            waitForRendering(page)
            compare(Math.round(bar.x + bar.width/2), width - Metrics.gap12)
            verify(bar.x > page.scrollView.mapToItem(page,page.scrollView.width,0).x)
            compare(bar.y,0)
        }
    }
    function test_track_stripes_ignore_work_and_disc_headers() {
        const list=createTemporaryObject(listMaker,this)
        fill(list.model,4)
        list.model.insert(0,Object.assign({},list.model.get(0),{trackId:0,rowKind:1,number:0,headerLabel:"Disc 1"}))
        list.model.insert(2,Object.assign({},list.model.get(0),{headerLabel:"A work"}))
        list.rows.forceLayout()
        let surfaces=[]
        tryVerify(()=> { surfaces=list.rows.contentItem.children.filter(child => child.rowIndex !== undefined && child.visible); return surfaces.length === 4 })
        compare(surfaces.map(surface=>surface.rowIndex).sort(),[0,1,2,3])
        const first=surfaces.find(surface=>surface.rowIndex===0)
        const second=surfaces.find(surface=>surface.rowIndex===1)
        verify(first.color !== second.color)
    }
    function test_inline_footer_receives_clicks_and_horizontal_drag_data() {
        return [{tag:"short album",count:4},{tag:"long playlist",count:2000}]
    }
    function test_inline_footer_receives_clicks_and_horizontal_drag(data) {
        const list=createTemporaryObject(listMaker,this)
        fill(list.model,data.count)
        list.rows.forceLayout()
        list.rows.positionViewAtEnd()
        const rail=findChild(list,"footerRail")
        tryVerify(()=>rail && rail.mapToItem(list,0,0).y < list.height)
        const before=footerClicks
        mouseClick(findChild(rail,"footerCard"),60,60)
        compare(footerClicks,before+1)
        compare(list.model.lastAction,"")
        mouseDrag(rail,800,100,-400,0,Qt.LeftButton)
        tryVerify(()=>rail.contentX > 100)
        rail.cancelFlick()
        list.rows.positionViewAtIndex(1,ListView.Center)
        waitForRendering(list)
        const row=list.rows.itemAtIndex(1)
        verify(row)
        const point=row.mapToItem(list,300,row.height/2)
        mouseClick(list,point.x,point.y,Qt.LeftButton,Qt.ControlModifier)
        compare(list.model.lastAction,"select")
        compare(list.model.lastRow,1)
    }
}
