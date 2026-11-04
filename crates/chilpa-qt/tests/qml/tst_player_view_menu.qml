import QtQuick
import QtTest
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Components" as Presentation

TestCase {
    id:test
    name:"PlayerViewMenu"
    when:windowShown
    visible:true;width:1024;height:600
    Item {id:trigger;width:28;height:28;x:600;y:550}
    Presentation.PlayerViewMenu {id:menu;parent:trigger}
    function cleanup(){menu.close();Sources.displays={};Sources.show({panels:[]})}
    function test_approved_display_is_in_the_visible_menu_and_refreshes(){
        Sources.displays={fixture:"file:///fixture.qml"}
        Sources.show({panels:[{id:"fixture",name:"Fixture display"}]})
        menu.show()
        tryCompare(menu,"opened",true)
        compare(menu.shown.filter(row=>row.entry.text==="Fixture display").length,1)
        verify(menu.entryItems.some(item=>item.entry?.text==="Fixture display"))
        menu.close()
        Sources.displays={};Sources.revision++
        menu.show()
        compare(menu.shown.filter(row=>row.entry.text==="Fixture display").length,0)
    }
}
