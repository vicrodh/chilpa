import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/sources/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/sources/qml/Components"

TestCase {
    id: test
    name: "MusicServers"
    when: windowShown
    visible: true
    width: 960
    height: 600
    property int backgroundPresses: 0
    MouseArea { anchors.fill: parent; onClicked: test.backgroundPresses++ }
    property var rows: []
    MusicServers { id: table; width: 650; height: 400; model: rows }
    ListModel { id: facadeRows }
    ServerConnectionDialog { id: dialog }
    SignalSpy { id: addSpy; target: table; signalName: "addRequested" }
    SignalSpy { id: actionSpy; target: table; signalName: "actionRequested" }
    function init() {
        test.width=960; test.height=600; backgroundPresses=0
        Servers.calls=[]; Servers.refreshes=0; Servers.show({step:"idle"})
        dialog.serverId=-1; dialog.preview=null
        addSpy.clear(); actionSpy.clear(); const fixture=[]
        for (let i=0;i<80;i++) fixture.push({key:"sources.server/"+i,title:"Server "+i,description:"Connection details",action:"Connected",value:JSON.stringify({icon:"navidrome",product:"Navidrome 0.64.2",tracks:i}),options:["Edit…","Disable","Sync now","Remove…"],optionValues:["edit","disable","sync","remove"]})
        rows=fixture
        table.model=rows
    }
    function cleanup() { dialog.close(); tryCompare(dialog,"visible",false) }
    function open() { dialog.open(); tryCompare(dialog,"opened",true) }
    function viewport() { return findChild(dialog.contentItem,"serverFormViewport") }
    function field(name) { return findChild(dialog.contentItem,name) }
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
    function test_add_and_edit_fit_and_block_background() {
        open()
        verify(viewport().height>0)
        verify(dialog.width<=test.width)
        verify(dialog.height<=test.height)
        mouseClick(dialog.contentItem,dialog.contentItem.width-5,5)
        compare(backgroundPresses,0)
        dialog.close(); tryCompare(dialog,"visible",false)
        dialog.show(42); tryCompare(dialog,"opened",true)
        compare(Servers.calls[Servers.calls.length-1],["begin","edit/42"])
        compare(field("serverLabel").text,"Saved server")
        compare(field("serverAddress").text,"http://music.local")
        compare(field("serverAddress").width,viewport().width)
        field("serverAddress").text="http://updated.local"
        field("serverContinue").clicked()
        compare(Servers.calls[Servers.calls.length-1],["address","http://updated.local"])
        mouseClick(test,955,595)
        compare(backgroundPresses,0)
        tryCompare(dialog,"visible",false)
        compare(Servers.calls[Servers.calls.length-1],["cancel"])
    }
    function test_long_libraries_scroll_and_save_selection() {
        dialog.show(7); tryCompare(dialog,"opened",true)
        const choices=[]
        for (let i=0;i<40;i++) choices.push({id:"lib"+i,name:"Library "+i,detail:""})
        Servers.show({step:"libraries",kind:"jellyfin",choices:choices,selectedLibraries:["lib2"]})
        tryVerify(()=>viewport().contentHeight>viewport().height)
        verify(viewport().height>0)
        verify(dialog.height<=test.height)
        test.height=360
        tryVerify(()=>dialog.height<=test.height)
        verify(viewport().height>0)
        field("serverContinue").clicked()
        const sent=Servers.calls[Servers.calls.length-1]
        compare(sent[0],"finish")
        compare(sent[1].libraries,["lib2"])
        compare(sent[1].label,"Saved server")
        Servers.show({step:"done"})
        tryCompare(dialog,"visible",false)
    }
    function test_secrets_are_destroyed_and_polling_stops_on_close() {
        open()
        Servers.show({step:"signin",kind:"jellyfin",busy:false})
        field("serverSecret").text="fixture-secret"
        field("serverUser").text="fixture-user"
        field("serverContinue").clicked()
        compare(Servers.calls[Servers.calls.length-1],["signIn","fixture-user","fixture-secret",false])
        Servers.show({step:"code",kind:"jellyfin",busy:false,code:"1234"})
        tryVerify(()=>Servers.refreshes>0)
        dialog.close(); tryCompare(dialog,"visible",false)
        const count=Servers.refreshes
        wait(300)
        compare(Servers.refreshes,count)
        compare(field("serverSecret"),null)
        open(); Servers.show({step:"signin",kind:"jellyfin"})
        compare(field("serverSecret").text,"")
    }
}
