import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "../../qml/Components"

TestCase {
    id: test
    name: "TrackCredits"
    when: windowShown
    visible: true
    width: 1500
    height: 600
    property var document: ({artist_html:'<a href="0">Lead</a>',featured_html:'feat. <a href="1">Guest</a>',composer_html:'Composer: <a href="2">Writer</a>',html:'<a href="0">Lead</a> · feat. <a href="1">Guest</a>',links:[
        {name:"Lead",target:"source",key:"fixture native:artist:lead"},
        {name:"Guest",target:"source",key:"fixture native:artist:guest"},
        {name:"Writer",target:"source",key:"fixture native:artist:writer"}]})
    TrackListModel { id: musicModel }
    TrackList { id: list; width: 1450; height: 260; model: musicModel; rich: false; showAlbum: false }
    CreditLine { id: standalone; y: 300; width: 1000; height: 30; credit: "Lead"; trackPath: "fixture:track" }
    function init() {
        Prefs.trackComposers = false
        list.showArtist = true
        Navigation.went = []
        Sources.creditDocuments = {"fixture:track": document}
        musicModel.clear()
        musicModel.append({trackId:-1,rowKind:0,number:1,title:"Track",artist:"Lead",album:"Record",composer:"Writer",credits:JSON.stringify(document),duration:"3:00",plays:0,quality:"FLAC",tier:2,available:true,playing:false,selected:false,headerLabel:"",headerMeta:"",art:"",sourceInfo:"{}",sourceName:""})
        list.rows.forceLayout()
        tryVerify(() => list.rows.itemAtIndex(0)?.creditedTitle !== null && list.rows.itemAtIndex(0)?.creditedTitle !== undefined)
    }
    function cleanup() { Prefs.trackComposers = false; Sources.creditDocuments = ({}) }
    function pointFor(label, target) {
        for (let y = 1; y < label.height; y++)
            for (let x = 1; x < label.width; x++)
                if (label.linkAt(x,y) === target) return Qt.point(x,y)
        fail("missing visible credit link " + target)
        return Qt.point(0,0)
    }
    function clickLink(label, target, surface) {
        const p = pointFor(label,target)
        const at = label.mapToItem(surface,p.x,p.y)
        mouseClick(surface,at.x,at.y)
    }
    function test_featured_is_visible_by_the_title_and_composer_is_opt_in() {
        const title = findChild(list.rows.itemAtIndex(0),"titleCredit")
        verify(title.text.includes("Guest"))
        verify(!title.text.includes("Writer"))
        clickLink(title,"1",list)
        compare(Navigation.went,["source","fixture native:artist:guest"])
        Prefs.trackComposers = true
        verify(title.text.includes("Composer:"))
        clickLink(title,"2",list)
        compare(Navigation.went,["source","fixture native:artist:writer"])
    }
    function test_album_without_artist_column_keeps_featured_link_by_title() {
        list.showArtist = false
        const title = findChild(list.rows.itemAtIndex(0),"titleCredit")
        verify(title.text.includes("Guest"))
        clickLink(title,"1",list)
        compare(Navigation.went,["source","fixture native:artist:guest"])
    }
    function test_each_primary_and_featured_artist_has_its_own_destination() {
        clickLink(findChild(list.rows.itemAtIndex(0),"trackPerformingArtists"),"0",list)
        compare(Navigation.went,["source","fixture native:artist:lead"])
        clickLink(standalone,"1",standalone)
        compare(Navigation.went,["source","fixture native:artist:guest"])
        clickLink(standalone,"0",standalone)
        compare(Navigation.went,["source","fixture native:artist:lead"])
    }
}
