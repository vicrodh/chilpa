import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.about
import app.chilpa.controls
import app.chilpa.immersive

Item {
    id: probe
    property string outputDirectory: "OUTPUT_DIRECTORY"
    property bool restored: "RESTORED"
    property real minimumVisualPeak: "MINIMUM_VISUAL_PEAK"
    property real minimumWaveformHz: "MINIMUM_WAVEFORM_HZ"
    property int fixtureVolume: "FIXTURE_VOLUME"
    property bool fixtureMuted: "FIXTURE_MUTED"
    property bool firstRestored: restored
    property var mainWindow: null
    property int phase: 0
    property int mode: 0
    property int began: 0
    property real beganMs: 0
    property int ticks: 0
    property bool capturing: false
    property var counts: ({bars:0,waveform:0,energy:0,goniometer:0,oscilloscope:0,pack:0})
    property var before: ({})
    property real pulseBefore: 0
    readonly property var names: ["bars","waveform","energy","goniometer","oscilloscope","bars","bars"]
    readonly property var views: [{name:"spectrum",mode:3,scene:0},{name:"wave-bed",mode:6,scene:0},
        {name:"goniometer",mode:7,scene:0},{name:"oscilloscope",mode:8,scene:0},
        {name:"reactive-rings",mode:9,scene:0},{name:"plasma",mode:0,scene:1}]
    SettingsPage {id: audio; section:"audio"}
    function find(item,name){if(item.objectName===name)return item;for(const child of item.children || []){const found=find(child,name);if(found)return found}return null}
    function check(ok,message){if(!ok)throw new Error(message)}
    function peak(values){let result=0;for(let i=0;i<values.length;i++)result=Math.max(result,Math.abs(values[i]));return result}
    function begin(){began=ticks;beganMs=Date.now();before=Object.assign({},counts);pulseBefore=Pulse.ms}
    function capture(name,next){
        capturing=true
        mainWindow.contentItem.grabToImage(image=>{
            if(!image.saveToFile(outputDirectory+"/"+name+".png")){console.error("SMOKE FAIL Main visualizer: capture "+name);Qt.exit(1)}
            capturing=false
            next()
        })
    }
    function report(name){
        const fresh={};for(const key of Object.keys(counts))fresh[key]=counts[key]-before[key]
        const engine=typeof Visualizer.diagnostics==="function" ? JSON.parse(Visualizer.diagnostics()):null
        console.log("SMOKE Main visualizer stage "+JSON.stringify({name:name,seconds:(Date.now()-beganMs)/1000,fresh:fresh,pulse:Pulse.ms-pulseBefore,playing:Player.playing,paused:Player.paused,volume:Player.volume,muted:Player.muted,bars:Array.from(Visualizer.bars),energy:Array.from(Visualizer.energy),engine:engine}))
    }
    function selectCompact(next){mode=next;find(mainWindow.contentItem,"listenNow").visualizationMode=next;begin()}
    function selectImmersive(next){mode=next;Immersive.openView(0,views[mode].mode,0);Immersive.chooseScene(views[mode].scene);begin()}
    Connections {
        target:Visualizer
        function onBarsChanged(){probe.counts.bars++}
        function onWaveformChanged(){probe.counts.waveform++}
        function onEnergyChanged(){probe.counts.energy++}
        function onGoniometerChanged(){probe.counts.goniometer++}
        function onOscilloscopeChanged(){probe.counts.oscilloscope++}
        function onPackJsonChanged(){probe.counts.pack++}
    }
    Timer {
        interval:75;repeat:true;running:probe.phase<5
        onTriggered:{
            try {
                if(probe.capturing)return
                probe.ticks++
                if(probe.mainWindow && JSON.parse(Onboarding.doc || "{}").open){Onboarding.command("close");return}
                if(probe.phase===0){
                    const component=Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
                    probe.check(component.status===Component.Ready,component.errorString())
                    probe.mainWindow=component.createObject(null)
                    probe.mainWindow.width=1280;probe.mainWindow.height=800
                    if(probe.restored)Navigation.go("listenlist",0)
                    probe.phase=1;return
                }
                if(probe.phase===1){
                    if(!Player.path.endsWith("fixture.wav"))return
                    audio.set("audio.device","file:capture");Player.toggle();if(!probe.restored)Navigation.go("listenlist",0)
                    probe.phase=2;return
                }
                if(probe.phase===2){
                    const toggle=probe.find(probe.mainWindow.contentItem,"queueVisualizationToggle")
                    if(!Player.playing || Player.paused || !toggle || !toggle.visible || !toggle.enabled)return
                    Player.chooseVolume(probe.fixtureVolume)
                    Player.chooseMuted(probe.fixtureMuted)
                    if(probe.firstRestored){probe.mode=5;probe.begin()}
                    else{toggle.clicked();probe.selectCompact(0)}
                    probe.phase=3;return
                }
                if(probe.ticks-probe.began<20)return
                probe.check(Player.playing && !Player.paused,"playback stopped")
                if(probe.phase===3){
                    probe.report("compact-"+probe.mode)
                    probe.check(probe.counts[probe.names[probe.mode]]-probe.before[probe.names[probe.mode]]>3,"no fresh compact stream "+probe.mode)
                    probe.check(probe.peak(Visualizer[probe.names[probe.mode]])>=probe.minimumVisualPeak,"compact signal was attenuated below the usable visual range")
                    if(probe.mode===1)probe.check((probe.counts.waveform-probe.before.waveform)*1000/(Date.now()-probe.beganMs)>=probe.minimumWaveformHz,"distinct waveform updates fell below the required cadence")
                    if(probe.mode!==3 && probe.mode!==4)probe.check(Pulse.ms>probe.pulseBefore,"compact animation pulse stopped")
                    probe.capture("compact-"+probe.mode,()=>{
                        if(probe.firstRestored){probe.firstRestored=false;probe.selectCompact(0);return}
                        if(probe.mode<6){probe.selectCompact(probe.mode+1);return}
                        probe.find(probe.mainWindow.contentItem,"queueVisualizationToggle").clicked()
                        probe.selectImmersive(0);probe.phase=4
                    });return
                }
                probe.report("immersive-"+probe.views[probe.mode].name)
                probe.check(Immersive.open,"immersive did not open")
                probe.check(probe.counts.bars-probe.before.bars>3 && probe.counts.pack-probe.before.pack>3,"no fresh immersive audio")
                const scope=probe.views[probe.mode].mode
                if(scope===7 || scope===8)probe.check(probe.counts[scope===7 ? "goniometer":"oscilloscope"]-probe.before[scope===7 ? "goniometer":"oscilloscope"]>3,"immersive scope feed stopped")
                else probe.check(Pulse.ms>probe.pulseBefore,"immersive animation pulse stopped")
                probe.capture("immersive-"+probe.views[probe.mode].name,()=>{
                    if(probe.mode+1<probe.views.length){probe.selectImmersive(probe.mode+1);return}
                    probe.phase=5;Immersive.open=false;Player.clearQueue()
                    console.log("SMOKE Main visualizer completed");Qt.exit(0)
                })
            }catch(error){console.error("SMOKE FAIL Main visualizer: "+error);Qt.exit(1)}
        }
    }
    Timer {interval:60000;running:true;onTriggered:{console.error("SMOKE FAIL Main visualizer: timeout phase="+probe.phase+" playing="+Player.playing+" paused="+Player.paused);Qt.exit(1)}}
}
