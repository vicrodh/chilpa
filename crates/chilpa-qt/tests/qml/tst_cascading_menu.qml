import QtQuick
import QtTest
import "../../qml/Components"
TestCase {
    id: test
    name: "CascadingMenu"
    when: windowShown
    visible: true
    width: 960
    height: 600
    CascadingMenu {
        id: menu
        entries: [{text:"Play",act:"play"},{text:"Extra queues",submenu:true,children:[{text:"New queue",act:"new"},{text:"Add to Jazz",act:"add"}]}]
    }
    property int outsideClicks: 0
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.LeftButton | Qt.RightButton; onClicked: test.outsideClicks++ }
    SignalSpy { id: chosen; target: menu; signalName: "actionChosen" }
    function cleanup() { menu.close(); chosen.clear(); outsideClicks=0; mouseMove(test,10,550) }
    function openBranch(x) {
        menu.x=x; menu.y=100; menu.open()
        tryCompare(menu,"opened",true)
        const item=menu.entryItems.find(i => i.entry && i.entry.submenu)
        waitForRendering(item)
        mouseMove(item,item.width/2,item.height/2)
        const branch=findChild(menu,"menuBranch")
        tryCompare(branch,"opened",true)
        return branch
    }
    function test_hover_child_stays_open_and_selection_dispatches_once() {
        const branch=openBranch(100)
        const item=branch.entryItems.find(i=>i.entry && i.entry.act==="add")
        waitForRendering(item)
        mouseMove(item,item.width/2,item.height/2)
        wait(500)
        verify(menu.opened && branch.opened)
        const parentRow=menu.entryItems.find(i=>i.entry && i.entry.submenu)
        mouseMove(parentRow,parentRow.width/2,parentRow.height/2)
        wait(500)
        verify(menu.opened && branch.opened)
        mouseClick(item,item.width/2,item.height/2)
        compare(chosen.count,1)
        compare(chosen.signalArguments[0][0].act,"add")
        tryCompare(menu,"visible",false)
        tryCompare(branch,"visible",false)
    }
    function test_area_it_covered_takes_clicks_after_a_choice() {
        menu.x=100; menu.y=100; menu.open()
        tryCompare(menu,"opened",true)
        const item=menu.entryItems.find(i => i.entry && i.entry.act==="play")
        waitForRendering(item)
        const spot=item.mapToItem(test,item.width/2,item.height/2)
        mouseMove(item,item.width/2,item.height/2)
        mouseClick(item,item.width/2,item.height/2)
        compare(chosen.count,1)
        tryCompare(menu,"visible",false)
        mouseClick(test,spot.x,spot.y)
        compare(outsideClicks,1)
        mouseClick(test,spot.x,spot.y,Qt.RightButton)
        compare(outsideClicks,2)
    }
    function test_outside_click_is_consumed() {
        openBranch(100)
        mouseClick(test,10,550)
        tryCompare(menu,"visible",false)
        compare(outsideClicks,0)
    }
    Component { id: tallMenu; CascadingMenu {} }
    function test_long_branch_fits_at_bottom_edge() {
        const children=[]
        for (let i=0;i<40;i++) children.push({text:"Queue "+i,act:"add:"+i})
        const popup=createTemporaryObject(tallMenu,test,{x:700,y:550,entries:[{text:"Extra queues",submenu:true,children:children}]})
        popup.open(); tryCompare(popup,"opened",true)
        const row=popup.entryItems.find(i=>i.entry && i.entry.submenu)
        waitForRendering(row); mouseMove(row,row.width/2,row.height/2)
        const branch=findChild(popup,"menuBranch")
        tryCompare(branch,"opened",true)
        waitForRendering(branch.contentItem)
        const origin=branch.contentItem.mapToItem(test,0,0)
        verify(origin.y>=0)
        verify(origin.y+branch.availableHeight<=test.height)
        verify(branch.contentItem.contentHeight>branch.availableHeight)
        popup.close()
    }
    function test_left_edge_escape_and_leaving_both() {
        let branch=openBranch(700)
        verify(branch.x < 0)
        mouseMove(test,10,550)
        wait(500)
        verify(!menu.visible && !branch.visible)
        branch=openBranch(100)
        keyClick(Qt.Key_Escape)
        tryCompare(branch,"visible",false)
    }
}
