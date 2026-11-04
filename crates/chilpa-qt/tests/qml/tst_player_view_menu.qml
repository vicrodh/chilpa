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
    function test_right_edge_reserves_a_child_and_waits_for_the_pointer(){
        trigger.x=980
        menu.show();tryCompare(menu,"opened",true)
        const row=menu.entryItems.find(item=>item.entry?.act==="layout:" || item.entry?.children?.some(child=>child.act?.startsWith("layout:")))
        verify(row!==undefined)
        waitForRendering(row);mouseMove(test,10,10);mouseMove(row,row.width/2,row.height/2)
        const branch=findChild(menu,"menuBranch");tryCompare(branch,"opened",true)
        verify(branch.x>0)
        const corner=branch.contentItem.mapToItem(test,branch.availableWidth,0)
        verify(corner.x<=test.width)
        mouseMove(test,10,10);wait(1100)
        verify(menu.opened && branch.opened)
        trigger.x=600
    }
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
