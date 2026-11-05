import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.about
import app.chilpa.immersive
import app.chilpa.controls

Item {
    id: probe
    property int mode: "BENCHMARK_MODE"
    property int volume: "FIXTURE_VOLUME"
    property bool muted: "FIXTURE_MUTED"
    property var mainWindow: null
    property int phase: 0
    property int step: 0
    property var counts: ({bars:0,waveform:0,energy:0,goniometer:0,oscilloscope:0})
    property var before: ({})
    property var frames: ({count:0})
    property int framesBefore: 0
    property real beganMs: 0
    SettingsPage { id: audio; section: "audio" }
    function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const result=find(child,name);if(result)return result}return null}
    function mark(name){console.log("SMOKE BENCH phase "+name)}
    function wait(ms){stage.interval=ms;stage.restart()}
    Connections {
        target: probe.mainWindow
        function onFrameSwapped(){probe.frames.count++}
    }
    Connections {
        target: Visualizer
        function onBarsChanged(){probe.counts.bars++}
        function onWaveformChanged(){probe.counts.waveform++}
        function onEnergyChanged(){probe.counts.energy++}
        function onGoniometerChanged(){probe.counts.goniometer++}
        function onOscilloscopeChanged(){probe.counts.oscilloscope++}
    }
    Timer {
        interval: 75; repeat: true; running: probe.phase < 3
        onTriggered: {
            try {
                if(probe.mainWindow && JSON.parse(Onboarding.doc || "{}").open){Onboarding.command("close");return}
                if(probe.phase===0){
                    const component=Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
                    if(component.status!==Component.Ready)throw new Error(component.errorString())
                    probe.mainWindow=component.createObject(null)
                    probe.mainWindow.width=1280;probe.mainWindow.height=800
                    Navigation.go("listenlist",0);probe.phase=1;return
                }
                if(probe.phase===1){
                    if(!Player.path.endsWith("fixture.wav"))return
                    audio.set("audio.device","file:capture");Player.toggle();probe.phase=2;return
                }
                const toggle=probe.find(probe.mainWindow.contentItem,"queueVisualizationToggle")
                if(!Player.playing || Player.paused || !toggle || !toggle.enabled || !toggle.visible)return
                Player.chooseVolume(probe.volume);Player.chooseMuted(probe.muted)
                const view=probe.find(probe.mainWindow.contentItem,"listenNow")
                if(view.visualizationMode!==probe.mode)view.visualizationMode=probe.mode
                if(!toggle.checked)toggle.clicked()
                probe.phase=3;probe.mark("warmup");probe.wait(3000)
            }catch(error){console.error("SMOKE FAIL visualizer benchmark: "+error);Qt.exit(1)}
        }
    }
    Timer {
        id: stage
        repeat: false
        onTriggered: {
            try {
                const toggle=probe.find(probe.mainWindow.contentItem,"queueVisualizationToggle")
                if(probe.step===0){
                    probe.before=Object.assign({},probe.counts)
                    probe.framesBefore=probe.frames.count;probe.beganMs=Date.now()
                    probe.mark("active");probe.wait(12000)
                }else if(probe.step===1){
                    const fresh={};for(const key of Object.keys(probe.counts))fresh[key]=probe.counts[key]-probe.before[key]
                    console.log("SMOKE BENCH updates "+JSON.stringify(fresh))
                    console.log("SMOKE BENCH presents "+JSON.stringify({frames:probe.frames.count-probe.framesBefore,seconds:(Date.now()-probe.beganMs)/1000}))
                    if(!Player.playing || Player.paused)throw new Error("playback stopped during measurement")
                    const stream=["bars","waveform","energy","goniometer","oscilloscope","bars","bars"][probe.mode]
                    if(fresh[stream]<3)throw new Error("visible visualizer did not receive audio")
                    toggle.clicked();probe.mark("warmup");probe.wait(1000)
                }else if(probe.step===2){
                    if(JSON.parse(Visualizer.diagnostics()).enabled)throw new Error("hidden visualization kept capture enabled")
                    probe.mark("idle");probe.wait(5000)
                }else if(probe.step===3){
                    toggle.clicked();Player.toggle();probe.mark("warmup");probe.wait(3000)
                }else if(probe.step===4){
                    if(!Player.paused)throw new Error("player did not pause")
                    probe.mark("paused");probe.wait(5000)
                }else{
                    Player.clearQueue();console.log("SMOKE Main visualizer benchmark completed");Qt.exit(0);return
                }
                probe.step++
            }catch(error){console.error("SMOKE FAIL visualizer benchmark: "+error);Qt.exit(1)}
        }
    }
    Timer { interval:60000;running:true;onTriggered:{console.error("SMOKE FAIL visualizer benchmark: timeout");Qt.exit(1)} }
}
