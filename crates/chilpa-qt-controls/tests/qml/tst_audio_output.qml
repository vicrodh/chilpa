import QtQuick
import QtTest
import app.chilpa
import app.chilpa.controls
import "../../qml/Components"
import "../../qml/Patterns"
import "../../qml/Gallery"

TestCase {
    id: test
    name: "AudioOutputDetails"
    when: windowShown
    visible: true
    width: 1100
    height: 850
    AudioCheck { id: check; width: 650; document: ({canCheck:true,playing:false,busy:false,name:"Fixture DAC"}) }
    SignalSpy { id: commands; target:check; signalName:"command" }
    WindowControls { id: controls; window: test.Window.window }
    Item { id: invoking; objectName: "invokingControl"; activeFocusOnTab: true }
    QtObject { id: owner; property bool visible: true; property int visibility: Window.Windowed }
    Component { id: galleryExample; GalleryAudioOutput {} }
    function test_gallery_states_and_panel_preview_never_request_real_equipment() {
        const gallery = createTemporaryObject(galleryExample,test)
        verify(gallery)
        findChild(gallery,"audioGalleryPreview").clicked()
        const panel = findChild(gallery,"audioOutputPanel")
        tryCompare(panel,"opened",true)
        compare(AudioOutput.commands.length,0)
        verify(!findChild(panel,"audioOutputRefresh").running)
        panel.close()
    }
    function init() {
        AudioOutput.commands = []
        commands.clear()
        check.document = ({canCheck:true,playing:false,busy:false,name:"Fixture DAC",selection:"fixture:target",inspection:{identity:{generation:"revision-1"}}})
    }
    function cleanup() { AudioOutput.shown = false; wait(0) }
    function test_check_is_explicit_and_keeps_the_requested_tuple_and_mode() {
        compare(commands.count,0)
        findChild(check,"audioCheckFormat").picked("s24_in32")
        findChild(check,"audioCheckRate").picked("96000")
        findChild(check,"audioCheckChannels").picked("2")
        findChild(check,"audioCheckMode").picked("true")
        const start = findChild(check,"audioCheckStart")
        start.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(commands.count,1)
        const args = commands.signalArguments[0]
        compare(args[0],"check")
        const request = JSON.parse(args[1])
        compare(request.output,"fixture:target"); compare(request.generation,"revision-1")
        compare(request.format.sample_format,"s24_in32")
        compare(request.format.sample_rate_hz,96000)
        compare(request.format.channels,2)
        compare(request.exclusive,true)
    }
    function test_an_explicit_output_selection_refreshes_the_existing_player_controls() {
        const reads = Player.switchReads
        const casts = Cast.refreshes
        AudioOutput.outputSelected()
        compare(Player.switchReads,reads + 1)
        compare(Cast.refreshes,casts + 1)
    }
    function test_playback_and_a_running_job_block_another_check() {
        check.document = ({canCheck:true,playing:true,busy:false})
        compare(findChild(check,"audioCheckStart").enabled,false)
        check.document = ({canCheck:true,playing:false,busy:true,checking:true})
        compare(findChild(check,"audioCheckStart").enabled,false)
        const cancel = findChild(check,"audioCheckCancel")
        verify(cancel.visible)
        cancel.clicked()
        compare(commands.signalArguments[0][0],"cancel")
    }
    function test_shared_panel_releases_its_loaded_view_and_stops_polling() {
        AudioOutput.snapshot = JSON.stringify({outputs:[],now:{available:false,summary:"Nothing playing"}})
        AudioOutput.show("now")
        const host = findChild(controls,"audioOutputHost")
        tryVerify(() => host.item !== null)
        tryCompare(host.item,"opened",true)
        compare(host.item.page,"now")
        host.item.close()
        tryCompare(AudioOutput,"shown",false)
        tryCompare(host,"item",null)
        const reads = AudioOutput.reads
        wait(600)
        compare(AudioOutput.reads,reads)
    }
    function test_a_hidden_or_minimized_owner_stops_visible_refresh_and_escape_returns_focus() {
        invoking.forceActiveFocus()
        AudioOutput.show("equipment")
        const host = findChild(controls,"audioOutputHost")
        tryVerify(() => host.item && host.item.opened)
        host.item.ownerWindow = owner
        owner.visible = true; owner.visibility = Window.Windowed
        const timer = findChild(host.item,"audioOutputRefresh")
        tryCompare(timer,"running",true)
        owner.visibility = Window.Minimized
        tryCompare(timer,"running",false)
        const reads = AudioOutput.reads; wait(600); compare(AudioOutput.reads,reads)
        owner.visibility = Window.Windowed; owner.visible = false
        tryCompare(timer,"running",false)
        owner.visible = true; tryCompare(timer,"running",true)
        keyClick(Qt.Key_Escape)
        tryCompare(AudioOutput,"shown",false)
        tryCompare(host,"item",null)
        verify(invoking.activeFocus)
    }
}
