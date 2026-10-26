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
        DeviceSync.music = ({})
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
    function test_storage_events_follow_surface_visibility_without_idle_polling() {
        const watcher = child("deviceStorageEvents")
        verify(watcher.active)
        watcher.changed()
        compare(DeviceSync.lastAction, ["connectionsChanged"])
        page.visible = false
        verify(!watcher.active)
        compare(child("deviceProgressPoll").running, false)
        page.visible = true
        verify(watcher.active)
        compare(child("deviceProgressPoll").running, false)
    }
    function test_detected_card_uses_the_relative_folder_without_a_folder_dialog() {
        DeviceSync.setStatus({destination:"volume:card-fixture"})
        page.creating = true
        wait(20)
        verify(child("deviceManagedFolder").visible)
        verify(!child("deviceChooseFolder").visible)
        child("deviceProfileName").text = "Reader card"
        click("deviceCreateProfile")
        compare(DeviceSync.lastAction, ["create", "Reader card", "Chilpa"])
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
    function test_selection_opens_albums_then_tracks_and_returns_to_the_picker() {
        const selected = DeviceSync.data.options[0].checked
        const list = child("deviceSelections")
        const open = findChild(list.itemAtIndex(0),"deviceMusicOpen")
        mouseClick(open,open.width / 2,open.height / 2)
        compare(DeviceSync.lastAction,["browse","list-0"])
        const albums = child("deviceMusicList")
        tryCompare(albums,"count",1)
        tryVerify(() => albums.itemAtIndex(0) !== null)
        const album = findChild(albums.itemAtIndex(0),"deviceMusicOpen")
        album.forceActiveFocus()
        verify(findChild(album,"deviceMusicFocusRing").visible)
        keyClick(Qt.Key_Space)
        compare(DeviceSync.lastAction,["album","album:0"])
        tryCompare(albums,"count",2)
        tryVerify(() => albums.itemAtIndex(1) !== null)
        compare(albums.itemAtIndex(0).entry.title,"First song")
        compare(albums.itemAtIndex(1).entry.title,"Second song")
        click("deviceBrowserBack")
        compare(DeviceSync.music.level,"albums")
        click("deviceBrowserBack")
        bounds(child("deviceSelections"))
        compare(DeviceSync.data.options[0].checked,selected)
    }
    function test_browser_actions_keep_the_id_across_reused_artist_and_album_rows() {
        child("deviceSections").activated(1)
        const list = child("deviceMusicList")
        for (let cycle = 0; cycle < 12; ++cycle) {
            for (const kind of ["artist", "album", "playlist"]) {
                const id = kind + ":" + cycle
                DeviceSync.music = {source:"recorded",title:"Card",recorded:true,view:"artists",rows:[{id:id,title:kind,detail:"A group",kind:kind,open:true}],page:0,pages:1}
                tryVerify(() => list.itemAtIndex(0) !== null && list.itemAtIndex(0).entry?.id === id)
                const button = findChild(list.itemAtIndex(0), "deviceMusicOpen")
                verify(button !== null)
                button.clicked()
                compare(DeviceSync.lastAction, ["album", id])
            }
        }
    }
    function test_browsing_restores_a_preview_without_forcing_the_changes_tab() {
        DeviceSync.setStatus({phase:"review",plan:"prepared",label:"Ready to sync"})
        child("deviceSections").activated(0)
        DeviceSync.browse("list-0")
        DeviceSync.setStatus({phase:"browsing",busy:true})
        DeviceSync.setStatus({phase:"review",busy:false})
        compare(page.reviewing,false)
        bounds(child("deviceMusicList"))
        compare(DeviceSync.status.plan,"prepared")
    }
    function test_an_incoming_album_returns_from_history_and_clears_the_picker_filter() {
        const search = child("deviceSelectionSearch")
        search.text = "An older filter"
        DeviceSync.useTracks("[42,43]")
        compare(child("deviceSelectionSearch"),search)
        compare(search.text,"")
        search.text = "Another older filter"
        child("deviceSections").activated(1)
        compare(page.recorded,true)
        DeviceSync.useTracks("[42,43]")
        DeviceSync.closeBrowser()
        DeviceSync.setStatus({phase:"selecting",busy:true})
        DeviceSync.setStatus({phase:"idle",busy:false})
        compare(page.recorded,false)
        compare(page.reviewing,false)
        compare(child("deviceSelectionSearch").text,"")
        verify(child("deviceSelections").visible)
    }
    function test_synced_music_and_read_only_browser_controls() {
        child("deviceSections").activated(1)
        compare(DeviceSync.lastAction,["browse","recorded"])
        compare(DeviceSync.music.recorded,true)
        DeviceSync.music = Object.assign({},DeviceSync.music,{pages:2,can_back:false,notice:"Last synced with Chilpa. Files changed elsewhere may differ."})
        wait(20)
        click("deviceMusicNextPage")
        compare(DeviceSync.lastAction,["browserPage",1])
        child("deviceBrowserSearch").text = "Record"
        compare(DeviceSync.lastAction,["browserFilter","Record"])
        click("deviceBrowserReload")
        compare(DeviceSync.lastAction,["browse","recorded"])
        for (const size of [[360,558],[660,558],[1000,758]]) {
            page.width = size[0]; page.height = size[1]; wait(20)
            bounds(child("deviceMusicList"))
            bounds(child("deviceBrowserSearch"))
            bounds(child("devicePreview"))
        }
    }
    function test_collection_filters_and_paging_preserve_the_selection() {
        const selected = DeviceSync.data.options[0].checked
        child("deviceSelectionKind").picked("playlists")
        compare(DeviceSync.lastAction,["selectionKind","playlists"])
        DeviceSync.data = Object.assign({},DeviceSync.data,{selection_kind:"playlists",selection_pages:3,selection_page:0})
        wait(20)
        click("deviceSelectionsNextPage")
        compare(DeviceSync.lastAction,["selectionPage",1])
        child("deviceSelectionSearch").text = "Evening"
        compare(DeviceSync.lastAction,["filter","Evening"])
        compare(DeviceSync.data.options[0].checked,selected)
        DeviceSync.setStatus({busy:true,phase:"preparing"})
        compare(child("deviceSelectionKind").enabled,false)
        compare(child("deviceSelectionsNextPage").enabled,false)
    }
    function test_quick_review_explains_presence_and_keeps_approval_visible() {
        DeviceSync.setStatus({phase:"review",plan:"quick",label:"Ready to sync",removals:1,summary:"2 to copy · 1 to remove · 12 already present",review_note:"Quick review by size; copies and removals are verified when syncing."})
        for (const size of [[360,558],[1000,758]]) {
            page.width = size[0]; page.height = size[1]; wait(20)
            bounds(child("deviceChangeList")); bounds(child("deviceApproveRemovals")); bounds(child("deviceSyncNow"))
            compare(child("deviceSyncNow").enabled,false)
        }
    }
    function test_artist_album_track_and_playlist_categories_use_the_facade() {
        child("deviceSections").activated(1)
        const selected = DeviceSync.data.options[0].checked
        for (const view of ["music","artists","albums","tracks","playlists"]) {
            child("deviceBrowserView").picked(view)
            compare(DeviceSync.lastAction,["browserView",view])
            compare(DeviceSync.data.options[0].checked,selected)
            for (const size of [[360,558],[1000,758]]) {
                page.width = size[0]; page.height = size[1]; wait(20)
                bounds(child("deviceMusicList")); bounds(child("deviceBrowserView")); bounds(child("deviceBrowserSearch"))
            }
        }
    }
    function test_visual_review() {
        if (!captureVisuals) return
        page.width = 900; page.height = 760
        wait(40)
        grabImage(page).save("/tmp/chilpa-dap-ui-dark.png")
        DeviceSync.browse("list-0"); wait(40)
        grabImage(page).save("/tmp/chilpa-dap-albums-dark.png")
        DeviceSync.music = Object.assign({},DeviceSync.music,{view:"artists",rows:[{id:"artist:0",title:"An artist",detail:"12 albums",kind:"artist",open:true}]}); wait(40)
        grabImage(page).save("/tmp/chilpa-dap-artists-dark.png")
        DeviceSync.openBrowserAlbum("album:0"); wait(40)
        grabImage(page).save("/tmp/chilpa-dap-tracks-dark.png")
        ThemeModel.palette = Object.assign({},ThemeModel.palette,{"surface.window":"#fcfcfc","surface.pane":"#fcfcfc","surface.panel":"#f4f5f6","surface.raised":"#ffffff","text.primary":"#232629","text.secondary":"#707d8a","text.tertiary":"#9aa3ab","stroke.hairline":"#dddddd","fill.field":"#eeeeee","state.selected":"#e7e7e7","state.stripe":"#f4f4f4","row.selected":"#f3e5e6","accent":"#a3161f"})
        ThemeModel.rev++; wait(40)
        grabImage(page).save("/tmp/chilpa-dap-tracks-light.png")
        child("deviceSections").activated(1); wait(40)
        grabImage(page).save("/tmp/chilpa-dap-synced-light.png")
        child("deviceSections").activated(0); wait(40)
        grabImage(page).save("/tmp/chilpa-dap-ui-light.png")
        DeviceSync.setStatus({phase:"review",plan:"review",label:"Ready to sync",summary:"2 to copy · 0 to update · 1 to remove",removals:1,required:"24 MiB"})
        wait(40)
        grabImage(page).save("/tmp/chilpa-dap-ui-review.png")
    }
}
