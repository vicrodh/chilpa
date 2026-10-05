import QtQuick
import QtTest
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../../qml/Patterns"

TestCase {
    name: "SourceHeaders"; when:windowShown; visible:true; width:1100; height:800
    SourcePageHeader { id:header; width:parent.width; info:({kind:"album",title:"Album",quality_label:"WAV 24/96",quality_tier:3}) }
    SourceShelf { id:shelf; y:header.height; width:parent.width; wrap:true; section:({title:"Releases",key:"fixture native:artist:1|albums",more:"page:2",error:"Loading"}) }
    function findText(item,text) {
        if(item.text===text) return item
        for(const child of item.children || []) { const found=findText(child,text); if(found) return found }
        return null
    }
    function test_technical_quality_is_visible_before_playback() {
        const label=findText(header,"WAV 24/96")
        verify(label); verify(label.visible)
        header.info=Object.assign({},header.info,{quality_label:"WAV 16/44.1",quality_tier:2})
        verify(findText(header,"WAV 16/44.1").visible)
    }
    function test_more_is_below_the_content_and_centered() {
        const button=findText(shelf,"Load more")
        verify(button)
        compare(Math.round(button.x+button.width/2),Math.round(shelf.width/2))
        verify(button.y>findText(shelf,"Loading").y)
        button.clicked()
        compare(Sources.asked[Sources.asked.length-1],["section_more",shelf.activeKey,""])
    }
}
