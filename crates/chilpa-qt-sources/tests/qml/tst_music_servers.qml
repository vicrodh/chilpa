import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"

// The servers' table, and the wizard that adds and edits one: its kind,
// its connection, the test, its music and a last look. The facade is the
// test double of `Servers`, which writes down what it was asked.
TestCase {
    id: test
    name: "MusicServers"
    when: windowShown
    visible: true
    width: 960
    height: 700
    property int backgroundPresses: 0
    MouseArea { anchors.fill: parent; onClicked: test.backgroundPresses++ }
    property var rows: []
    MusicServers { id: table; width: 650; height: 400; model: rows }
    ListModel { id: facadeRows }
    ServerWizard { id: dialog }
    ServerRemoveDialog { id: removal }
    SignalSpy { id: addSpy; target: table; signalName: "addRequested" }
    SignalSpy { id: actionSpy; target: table; signalName: "actionRequested" }
    readonly property string plexLocation: "http://192.168.0.69:32469/DeviceDescription.xml"
    function init() {
        test.width=960; test.height=700; backgroundPresses=0
        Servers.calls=[]; Servers.refreshes=0; Servers.show({step:"idle"})
        dialog.serverId=-1; dialog.preview=null
        addSpy.clear(); actionSpy.clear(); const fixture=[]
        for (let i=0;i<80;i++) fixture.push({key:"sources.server/"+i,title:"Server "+i,description:"Connection details",action:"Connected",value:JSON.stringify({icon:"navidrome",product:"Navidrome 0.64.2",tracks:i}),options:["Edit…","Disable","Sync now","Remove…"],optionValues:["edit","disable","sync","remove"]})
        rows=fixture
        table.model=rows
    }
    function cleanup() { dialog.close(); tryCompare(dialog,"visible",false) }
    function open() { dialog.open(); tryCompare(dialog,"opened",true); waitForRendering(dialog.contentItem) }
    function field(name) { return findChild(dialog.contentItem,name) }
    function last() { return Servers.calls[Servers.calls.length-1] }
    function press(name) { verify(field(name).enabled, name + " is enabled"); field(name).clicked(); waitForRendering(dialog.contentItem) }
    function choose(kind) {
        mouseClick(field("serverKind_" + kind))
        compare(dialog.chosenKind, kind)
        press("serverContinue")
        compare(last(), ["begin", kind])
        compare(dialog.page, 1)
    }

    function test_facade_roles_hide_group_and_add_rows() {
        facadeRows.clear()
        facadeRows.append({key:"",title:"Servers",description:"",action:"",value:""})
        facadeRows.append({key:"sources.server/7",title:"Fixture",description:"Address",action:"Connected",value:'{"icon":"plex","product":"Plex 1.42","tracks":7}'})
        facadeRows.append({key:"sources.addServer",title:"Add",description:"",action:"",value:""})
        table.model=facadeRows
        const list=findChild(table,"musicServerList")
        tryVerify(()=>list.itemAtIndex(1)!==null)
        compare(list.itemAtIndex(1).name,"Fixture")
        compare(list.itemAtIndex(1).server.tracks,7)
        compare(list.itemAtIndex(0).height,0)
        compare(list.itemAtIndex(2).height,0)
    }
    function test_table_scrolls_and_routes_actions() {
        const list=findChild(table,"musicServerList")
        tryVerify(()=>list.itemAtIndex(0)!==null)
        verify(list.contentHeight>list.height)
        compare(list.itemAtIndex(0).server.product,"Navidrome 0.64.2")
        compare(list.itemAtIndex(0).values[0],"edit")
        const selector=findChild(list.itemAtIndex(0),"serverActions")
        compare(selector.options[0],"Edit…")
        selector.picked("sync")
        compare(actionSpy.signalArguments[0][0],"sources.server/0")
        compare(actionSpy.signalArguments[0][1],"sync")
        mouseClick(findChild(table,"addMusicServer"))
        compare(addSpy.count,1)
        list.positionViewAtEnd()
        tryVerify(()=>list.itemAtIndex(79)!==null)
        verify(list.contentItem.children.length<80)
    }

    // It starts at the kind, fits the window and keeps the background out.
    function test_the_wizard_starts_with_the_kind_and_blocks_the_background() {
        open()
        compare(dialog.page, 0)
        verify(dialog.width<=test.width && dialog.height<=test.height)
        verify(field("serverFormViewport").height>0)
        verify(!field("serverContinue").enabled, "a kind first")
        for (const kind of ["jellyfin", "subsonic", "plex", "dlna"]) verify(field("serverKind_" + kind) !== null, kind)
        compare(findChild(field("serverKind_dlna"), "serverKindIcon").sourceInfo.icon, "dlna", "the source icon of its kind")
        mouseClick(dialog.contentItem,dialog.contentItem.width-5,5)
        compare(backgroundPresses,0)
        choose("jellyfin")
        press("serverBack")
        compare(dialog.page, 0)
        compare(last(), ["cancel"], "back to the kinds starts over")
        mouseClick(test,955,695)
        compare(backgroundPresses,0)
    }

    // A server that signs in is tested with what was typed: its address,
    // then its sign-in, then its libraries; the result says what answered.
    function test_a_server_that_signs_in_is_tested_then_its_libraries_chosen() {
        open(); choose("jellyfin")
        verify(!field("serverContinue").enabled, "an address and a user first")
        field("serverAddress").text="http://jf.lan:8096"
        field("serverUser").text="fixture-user"
        field("serverSecret").text="fixture-secret"
        press("serverContinue")
        compare(last(), ["address","http://jf.lan:8096"])
        compare(dialog.page, 2)
        Servers.show({step:"signin",kind:"jellyfin",busy:false,quickConnect:false})
        compare(last(), ["signIn","fixture-user","fixture-secret",false], "signed in with what was typed, once")
        Servers.show({step:"libraries",kind:"jellyfin",label:"Living room",product:"Jellyfin 10.10.7",address:"http://jf.lan:8096",
            choices:[{id:"music",name:"Music",detail:""},{id:"books",name:"Audiobooks",detail:""}]})
        tryCompare(dialog, "connected", true)
        compare(field("serverResult").text, "Connected to Living room")
        compare(field("serverAdvice").text, "It offers 2 music libraries.")
        press("serverContinue")
        compare(dialog.page, 3)
        compare(dialog.chosen, ["music","books"], "every library, to begin with")
        field("serverLibrary_1").clicked()
        compare(dialog.chosen, ["music"])
        press("serverContinue")
        compare(dialog.page, 4)
        compare(field("serverLabel").text, "Living room", "the name the server gave itself")
        // Before it is added, the last look says how the first import goes.
        const note = field("serverNote")
        verify(note.visible, "a first import is explained before it starts")
        compare(note.variant, "info")
        compare(note.title, "Albums appear as they are read")
        verify(note.body.indexOf("first music source") > 0 && note.body.indexOf("plays meanwhile") > 0, note.body)
        press("serverContinue")
        compare(last()[0], "finish")
        compare(last()[1], {label:"Living room",libraries:["music"]})
        Servers.show({step:"done",kind:"jellyfin",product:"Living room"})
        tryCompare(dialog, "done", true)
        compare(field("serverContinue").text, "Close")
        verify(field("serverAdded").visible)
        verify(!field("serverNote").visible, "once added it says so instead")
    }

    // A failed test says what the server answered and what to do, and tries
    // again from there.
    function test_a_failed_test_says_what_to_do_and_tries_again() {
        open(); choose("subsonic")
        field("serverAddress").text="music.lan:4533"
        field("serverUser").text="lab"
        press("serverContinue")
        Servers.show({step:"address",kind:"subsonic",busy:false,error:"that address does not answer like a Subsonic server (HTTP 404)"})
        tryCompare(dialog, "failed", true)
        compare(field("serverResult").text, "That address does not answer like a Subsonic server (HTTP 404)")
        verify(field("serverAdvice").text.indexOf("Check the address") === 0, field("serverAdvice").text)
        compare(field("serverContinue").text, "Try again")
        press("serverContinue")
        compare(last(), ["address","music.lan:4533"])
        // An API key on a server that takes none is said before anything goes out.
        press("serverBack")
        field("serverUseKey").clicked()
        field("serverSecret").text="key"
        press("serverContinue")
        const before = Servers.calls.length
        Servers.show({step:"signin",kind:"subsonic",busy:false,apiKey:false})
        tryCompare(dialog, "failed", true)
        compare(Servers.calls.length, before, "nothing sent")
        verify(field("serverResult").text.indexOf("does not take an API key") > 0)
    }

    // A media server is chosen from a search or typed, then its folders are
    // opened level by level and the ones that hold the music chosen.
    function test_a_media_server_is_found_and_its_music_folder_chosen_across_levels() {
        open(); choose("dlna")
        Servers.show({step:"address",kind:"dlna",searching:false,choices:[{id:test.plexLocation,name:"Plex Media Server: Lab",detail:"Plex Media Server 1.43.5"}]})
        verify(field("serverUser") === null || !field("serverUser").visible, "nothing to sign in to")
        press("serverSearchAgain")
        compare(last(), ["begin","search"])
        waitForRendering(dialog.contentItem)
        mouseClick(field("serverFound_0"))
        compare(field("serverAddress").text, test.plexLocation)
        press("serverContinue")
        compare(last(), ["address",test.plexLocation])
        Servers.show({step:"libraries",kind:"dlna",label:"Plex Media Server: Lab",address:test.plexLocation,crumbs:[{id:"0",name:""}],
            choices:[{id:"music",name:"Music",detail:""},{id:"video",name:"Video",detail:""}]})
        tryCompare(dialog, "connected", true)
        verify(field("serverAdvice").text.indexOf("It shows 2 folders at its top") === 0)
        press("serverContinue")
        compare(dialog.page, 3)
        verify(!field("serverContinue").enabled, "a folder first")
        press("serverOpenFolder_0")
        compare(last(), ["pick","music"])
        Servers.show({step:"libraries",kind:"dlna",address:test.plexLocation,crumbs:[{id:"0",name:""},{id:"music",name:"Music"}],
            choices:[{id:"byfolder",name:"By Folder",detail:""},{id:"byalbum",name:"By Album",detail:""}],views:true})
        field("serverChooseFolder_0").clicked()
        compare(dialog.chosen, ["byfolder"])
        compare(dialog.names.byfolder, "Music › By Folder", "named by the way down to it")
        verify(!field("serverNote").visible, "one view is read once")
        field("serverChooseFolder_1").clicked()
        tryVerify(()=>field("serverNote").visible, 1000, "two views of one library read the same songs twice")
        compare(field("serverNote").title, "Several views of one library")
        compare(dialog.chosen, ["byfolder","byalbum"], "advised, not changed")
        field("serverChooseFolder_1").clicked()
        tryVerify(()=>!field("serverNote").visible)
        press("serverCrumb_0")
        compare(last(), ["pick","0"], "back up to the top")
        press("serverContinue")
        verify(field("serverNote").visible, "a media server's first import is explained too")
        press("serverContinue")
        compare(last()[1], {label:"Plex Media Server: Lab",libraries:["byfolder"]})
    }

    // A container that holds several views of one library is pointed out
    // under the choice, which stays as it was made.
    function test_a_folder_that_holds_views_is_pointed_out_and_the_choice_kept() {
        open(); choose("dlna")
        Servers.show({step:"libraries",kind:"dlna",address:test.plexLocation,crumbs:[{id:"0",name:""},{id:"top",name:"Music"}],
            choices:[{id:"section",name:"Music",detail:""},{id:"shared",name:"Shared Music",detail:""}],viewRoots:["section"]})
        dialog.page = 3
        waitForRendering(dialog.contentItem)
        verify(!field("serverNote").visible)
        field("serverChooseFolder_0").clicked()
        compare(dialog.chosen, ["section"], "the choice is kept")
        tryVerify(()=>field("serverNote").visible)
        compare(field("serverNote").title, "Several views of one library")
        verify(field("serverNote").body.indexOf("we suggest By Folder") > 0, field("serverNote").body)
        field("serverChooseFolder_1").clicked()
        verify(field("serverNote").visible, "still there while it is chosen")
        field("serverChooseFolder_0").clicked()
        tryVerify(()=>!field("serverNote").visible)
    }

    // Editing starts at the connection with the saved sign-in, and the
    // address goes to the server as typed.
    function test_editing_starts_at_its_connection_and_keeps_its_sign_in() {
        dialog.show(42); tryCompare(dialog,"opened",true); waitForRendering(dialog.contentItem)
        compare(Servers.calls[0],["begin","edit/42"])
        compare(dialog.page, 1)
        verify(!field("serverBack").visible, "its kind is fixed")
        compare(field("serverAddress").text,"http://music.local")
        verify(!field("serverUser").visible, "the saved sign-in stays")
        field("serverAddress").text="http://updated.local"
        press("serverContinue")
        compare(last(),["address","http://updated.local"])
        mouseClick(test,955,695)
        compare(backgroundPresses,0)
    }
    function test_long_libraries_scroll_and_save_selection() {
        dialog.show(7); tryCompare(dialog,"opened",true)
        const choices=[]
        for (let i=0;i<40;i++) choices.push({id:"lib"+i,name:"Library "+i,detail:""})
        Servers.show({step:"libraries",kind:"jellyfin",label:"Saved server",address:"http://music.local",choices:choices,editing:true})
        dialog.page = 3; dialog.chosen = ["lib2"]
        tryVerify(()=>field("serverFormViewport").contentHeight>field("serverFormViewport").height)
        test.height=420
        tryVerify(()=>dialog.height<=test.height)
        verify(field("serverFormViewport").height>0)
        press("serverContinue")
        verify(!field("serverNote").visible, "an edit is not a first import")
        press("serverContinue")
        compare(last()[0],"finish")
        compare(last()[1],{label:"Saved server",libraries:["lib2"]})
        Servers.show({step:"done",kind:"jellyfin",product:"Saved server"})
        tryCompare(dialog,"visible",false, 2000, "an edit that was saved closes")
    }
    // Removing a server keeps its dialog, covered and unanswerable, until
    // the removal ends; a failure stays readable, and success closes it.
    function test_removing_a_server_keeps_the_dialog_until_it_ends() {
        removal.serverId = 7
        removal.message = Servers.removal(7)
        removal.open(); tryCompare(removal, "opened", true)
        const find = name => findChild(removal.contentItem, name)
        verify(!find("serverRemoving").visible)
        find("serverRemoveConfirm").clicked()
        compare(last(), ["remove", 7])
        Servers.show({step:"idle", removing:{id:7, name:"Lab", state:"removing"}})
        tryVerify(()=>find("serverRemoving").visible)
        verify(find("serverRemovingText").text.indexOf("Removing Lab") === 0, find("serverRemovingText").text)
        verify(!find("serverRemoveConfirm").enabled && !find("serverRemoveCancel").enabled, "nothing to press under it")
        keyClick(Qt.Key_Escape)
        mouseClick(test, 955, 695)
        verify(removal.opened, "neither Esc nor a click outside closes it")
        // It fails: what failed, what to do, and words to copy.
        Servers.show({step:"idle", removing:{id:7, name:"Lab", state:"failed", title:"Lab could not be removed",
            advice:"The library stayed busy with another task. Nothing more was taken out; try again in a moment.", detail:"database is locked"}})
        tryVerify(()=>find("serverRemoveFailed").visible)
        verify(!find("serverRemoving").visible)
        compare(find("serverRemoveFailed").title, "Lab could not be removed")
        compare(find("serverRemoveFailed").variant, "error")
        compare(find("serverRemoveDetail").text, "database is locked")
        verify(find("serverRemoveDetail").selectByMouse && find("serverRemoveDetail").readOnly)
        compare(find("serverRemoveConfirm").text, "Try again")
        find("serverRemoveConfirm").clicked()
        compare(last(), ["remove", 7], "tried again")
        Servers.show({step:"idle", removing:{id:7, name:"Lab", state:"removing"}})
        Servers.show({step:"idle", removing:{id:7, name:"Lab", state:"done"}, notice:{seq:1, text:"Lab was removed"}})
        tryCompare(removal, "visible", false)
    }

    function test_secrets_are_destroyed_and_polling_stops_on_close() {
        open(); choose("jellyfin")
        field("serverSecret").text="fixture-secret"
        field("serverUser").text="fixture-user"
        field("serverAddress").text="http://jf.lan:8096"
        press("serverContinue")
        Servers.show({step:"code",kind:"jellyfin",busy:false,code:"1234"})
        tryVerify(()=>Servers.refreshes>0)
        compare(field("serverCode").text, "1234")
        dialog.close(); tryCompare(dialog,"visible",false)
        const count=Servers.refreshes
        wait(300)
        compare(Servers.refreshes,count)
        compare(field("serverSecret"),null)
        open(); choose("jellyfin")
        compare(field("serverSecret").text,"")
    }
}
