import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.immersive

// Opt-in observation only: the ordinary Main owns playback and view state.
Item {
    id: probe
    property var mainWindow: null
    property int samples: 0
    property var counts: ({bars:0,energy:0,waveform:0,goniometer:0,oscilloscope:0,pack:0})
    property var previous: ({})
    property real previousPulse: 0
    Component.onCompleted: {
        const component=Qt.createComponent("qrc:/qt/qml/app/chilpa/qml/Main.qml")
        if(component.status!==Component.Ready){console.error("CHILPA_VIZ_DIAG "+JSON.stringify({error:"Main component unavailable"}));Qt.exit(1);return}
        mainWindow=component.createObject(null)
        previousPulse=Pulse.ms
    }
    function peak(values){let out=0;for(let i=0;i<values.length;i++)out=Math.max(out,Math.abs(values[i]));return out}
    Connections {
        target:Visualizer
        enabled:probe.samples<180
        function onBarsChanged(){probe.counts.bars++}
        function onEnergyChanged(){probe.counts.energy++}
        function onWaveformChanged(){probe.counts.waveform++}
        function onGoniometerChanged(){probe.counts.goniometer++}
        function onOscilloscopeChanged(){probe.counts.oscilloscope++}
        function onPackJsonChanged(){probe.counts.pack++}
    }
    Timer {
        interval:1000;repeat:true;running:probe.samples<180
        onTriggered:{
            const changes={}
            for(const key of Object.keys(probe.counts))changes[key]=probe.counts[key]-(probe.previous[key] || 0)
            const state=typeof Visualizer.diagnostics==="function" ? JSON.parse(Visualizer.diagnostics()):{unavailable:true}
            console.log("CHILPA_VIZ_DIAG "+JSON.stringify({sample:probe.samples,engine:state,
                playing:Player.playing,paused:Player.paused,volume:Player.volume,quality:Player.qualityLabel,
                immersive:Immersive.open,mode:Immersive.mode,scene:Immersive.scene,
                pulse_delta:Pulse.ms-probe.previousPulse,changes:changes,
                peaks:{bars:probe.peak(Visualizer.bars),energy:probe.peak(Visualizer.energy),waveform:probe.peak(Visualizer.waveform)}}))
            probe.samples++;probe.previous=Object.assign({},probe.counts);probe.previousPulse=Pulse.ms
        }
    }
}
