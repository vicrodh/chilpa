import QtQuick
import QtTest
import app.chilpa.devices
import app.chilpa.kit
import "../../qml/Screens"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

TestCase {
    id: test
    name: "DeviceSyncSurface"
    when: windowShown
    visible: true
    width: 1280
    height: 900
    property var page: null
    property bool captureVisuals: false
    Component { id: surface; DeviceSyncScreen { Rectangle { anchors.fill: parent; color: Theme.color.surfaceWindow; z: -1 } } }
    function init() {
        DeviceSync.status = {phase:"idle",busy:false,profile:"profile",destination:"storage",label:"Choose music for this storage",remove_unselected:false,plan:"",removals:0,error:null}
        DeviceSync.data = {has_profiles:true,profile_folder:"Chilpa",storage_detail:"40 GiB available of 64 GiB",profile_names:["Player SD"],profile_values:["profile"],destination_names:["Player · Memory card"],destination_values:["storage"],destination_enabled:[true],destination_details:["40 GiB available"],options:selections(80),selected:"1 selection",can_review:true,change_count:3,page_count:2}
        DeviceSync.lastAction = []
        ThemeModel.palette = {"surface.window":"#1b1e20","surface.pane":"#1b1e20","surface.panel":"#232629","surface.raised":"#31363b","text.primary":"#fcfcfc","text.secondary":"#a1a9b1","text.tertiary":"#6e767e","stroke.hairline":"#34373a","fill.field":"#2b2e30","state.selected":"#34373a","state.stripe":"#212426","row.selected":"#3d2225","accent":"#d8323a","text.onAccent":"#ffffff","status.negative":"#e5606d"}
        ThemeModel.rev++
        page = createTemporaryObject(surface,test,{width:900,height:800})
        verify(page)
        wait(40)
    }
    function cleanup() { if (page) page.destroy(); page = null; wait(1) }
    function selections(count) {
        const rows = []
        for (let at = 0; at < count; at++) rows.push({id:"list-" + at,name:"Evening collection " + (at + 1),description:at % 2 ? "genre:Jazz year:1960..1989" : "24 tracks",checked:at === 0})
        return rows
    }
    function child(name) { const item = findChild(page,name); verify(item !== null,name); return item }
    function click(name) { const item = child(name); verify(item.visible,name); mouseClick(item,item.width / 2,item.height / 2) }
    function bounds(item) {
        const origin = item.mapToItem(page,0,0)
        verify(item.width > 0 && item.height > 0,item.objectName + " has a viewport: " + item.width + "x" + item.height + "; parent=" + item.parent.width + "x" + item.parent.height + "; container=" + item.parent.parent.width + "x" + item.parent.parent.height)
        verify(origin.x >= 0 && origin.y >= 0,item.objectName + " starts inside the page")
        verify(origin.x + item.width <= page.width + 1,item.objectName + " fits horizontally")
        verify(origin.y + item.height <= page.height,item.objectName + " fits vertically")
    }
    function test_geometry_keeps_selection_and_actions_in_the_viewport() {
        for (const size of [[360,558],[660,558],[1000,758]]) {
            page.width = size[0]; page.height = size[1]; wait(20)
            bounds(child("deviceSelections")); bounds(child("devicePreview")); bounds(child("deviceDestination")); bounds(child("deviceProfile"))
            const a = child("deviceProfile"), b = child("deviceDestination")
            verify(a.mapToItem(page,a.width,0).x <= b.mapToItem(page,0,0).x)
            DeviceSync.setStatus({phase:"error",error:{message:"The player is not responding. Reconnect it and try again. Its recovery information is preserved.",detail:"A USB transfer timed out.",recovery:true}})
            wait(20); bounds(child("deviceRecover")); bounds(child("deviceSelections"))
            DeviceSync.setStatus({phase:"idle",error:null})
        }
    }
    function test_removals_need_consent_for_the_current_preview() {
        DeviceSync.setStatus({phase:"review",plan:"first",label:"Ready to sync",removals:2,summary:"2 to copy · 2 to remove"})
        wait(20)
        compare(child("deviceSyncNow").enabled,false)
        click("deviceApproveRemovals")
        compare(child("deviceSyncNow").enabled,true)
        click("deviceSyncNow")
        compare(DeviceSync.lastAction,["apply","first",true])
        DeviceSync.setStatus({plan:"second"})
        wait(10)
        compare(child("deviceApproveRemovals").checked,false)
        compare(child("deviceSyncNow").enabled,false)
        keyClick(Qt.Key_Tab)
    }
    function test_reading_and_cancellation_use_the_same_task() {
        click("devicePreview"); compare(DeviceSync.lastAction,["preview",false])
        DeviceSync.setStatus({busy:true,phase:"preparing",label:"Reviewing changes…"})
        wait(20)
        compare(child("deviceProgressPoll").running,true)
        page.visible = false
        compare(child("deviceProgressPoll").running,false)
        const before = DeviceSync.polls
        wait(360); compare(DeviceSync.polls,before)
        page.visible = true
        tryVerify(() => DeviceSync.polls > before,800)
        click("deviceCancel"); compare(DeviceSync.lastAction,["cancel"])
        DeviceSync.setStatus({busy:false,phase:"cancelled",label:"Sync cancelled"})
        compare(child("deviceProgressPoll").running,false)
    }
    function test_selection_changes_keep_the_delegate_and_scroll_position() {
        const list = child("deviceSelections")
        list.contentY = 500
        wait(30)
        const at = list.indexAt(list.width / 2,list.contentY + list.height / 2)
        const row = list.itemAtIndex(at)
        verify(row)
        const position = list.contentY
        const rows = DeviceSync.data.options.slice()
        rows[at] = Object.assign({},rows[at],{checked:!rows[at].checked})
        DeviceSync.data = Object.assign({},DeviceSync.data,{options:rows})
        wait(30)
        compare(list.itemAtIndex(at),row)
        compare(list.contentY,position)
        const checkbox = findChild(row,"deviceSelectionCheck")
        mouseClick(checkbox,checkbox.width / 2,checkbox.height / 2)
        compare(DeviceSync.lastAction,["toggle","list-" + at])
    }
    function test_page_and_recovery_actions() {
        DeviceSync.setStatus({phase:"review",plan:"review",label:"Ready to sync"})
        wait(20)
        bounds(child("deviceChangeList"))
        click("deviceNextPage"); compare(DeviceSync.lastAction,["page",1])
        click("devicePreviousPage"); compare(DeviceSync.lastAction,["page",0])
        DeviceSync.setStatus({phase:"error",plan:"",error:{message:"An interrupted sync needs recovery.",detail:"",recovery:true}})
        wait(20); click("deviceRecover"); compare(DeviceSync.lastAction,["preview",true])
    }
    function test_pairing_retains_inputs_until_creation_succeeds() {
        click("deviceAddStorage"); wait(20)
        const name = child("deviceProfileName"), folder = child("deviceManagedFolder")
        name.text = "Travel card"; folder.text = "My music"
        click("deviceCreateProfile")
        compare(DeviceSync.lastAction,["create","Travel card","My music"])
        DeviceSync.setStatus({phase:"error",error:{message:"The player disconnected.",detail:"",recovery:false}})
        compare(name.text,"Travel card"); compare(folder.text,"My music")
        DeviceSync.profileCreated(); wait(10)
        bounds(child("deviceSelections"))
    }
    function test_visual_review() {
        if (!captureVisuals) return
        page.width = 900; page.height = 760
        wait(40)
        grabImage(page).save("/tmp/chilpa-dap-ui-dark.png")
        ThemeModel.palette = Object.assign({},ThemeModel.palette,{"surface.window":"#fcfcfc","surface.pane":"#fcfcfc","surface.panel":"#f4f5f6","surface.raised":"#ffffff","text.primary":"#232629","text.secondary":"#707d8a","text.tertiary":"#9aa3ab","stroke.hairline":"#dddddd","fill.field":"#eeeeee","state.selected":"#e7e7e7","state.stripe":"#f4f4f4","row.selected":"#f3e5e6","accent":"#a3161f"})
        ThemeModel.rev++; wait(40)
        grabImage(page).save("/tmp/chilpa-dap-ui-light.png")
        DeviceSync.setStatus({phase:"review",plan:"review",label:"Ready to sync",summary:"2 to copy · 0 to update · 1 to remove",removals:1,required:"24 MiB"})
        wait(40)
        grabImage(page).save("/tmp/chilpa-dap-ui-review.png")
    }
}
