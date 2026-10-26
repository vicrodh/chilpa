import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Patterns"

TitledFlow {
    title: "Audio and output"
    width: Metrics.settingsMaxWidth
    AudioQualityGlyph { tier: 2; preview: true }
    AudioQualityGlyph { tier: 3; preview: true }
    AudioQualityGlyph { tier: 4; preview: true }
    AudioFact { width: Metrics.confirmWidth; heading: "Endpoint readback"; value: "PCM · 96000 Hz · 24/32 bits · 2 channels"; note: "Read at opening; audibility is untested." }
    AudioNow { width: Metrics.confirmWidth; document: ({available:false,summary:"Nothing is playing. Equipment details remain available."}) }
    AudioNow {
        width: Metrics.confirmWidth
        document: ({available:true,summary:"No sample changes are reported in Chilpa; the downstream path is unverified.",
            source:"FLAC · 96000 Hz · 24 musical bits",processing:"Word widened without sample loss",
            output:"96000 Hz · PCM 24/32 · 2 channels",readback:"No physical readback is available for this route."})
    }
    AudioNow {
        width: Metrics.confirmWidth
        document: ({available:true,cast:true,summary:"Original audio sent to the receiver.",rendererStatus:"Paused",
            rendererError:"Receiver status is unavailable.",source:"FLAC · 44100 Hz",processing:"Local DSP is bypassed.",
            output:"Network transport",readback:"The physical DAC is not exposed."})
    }
    AudioEquipment { width: Metrics.confirmWidth; document: ({status:"Loading…",busy:true,formats:[],attributes:[],history:[]}) }
    AudioEquipment { width: Metrics.confirmWidth; document: ({status:"Unknown",inspection:{identity:{strong:false}},formats:[],attributes:[],history:[]}) }
    AudioEquipment {
        width: Metrics.confirmWidth
        document: ({status:"Details available",canCheck:true,inspection:{identity:{strong:true,stable_id:"Example DAC"}},
            formats:[{description:"PCM · 44100–96000 Hz · 24/32 bits · 2 channels",scope:"Declared by the hardware driver"}],
            attributes:[],history:[{requested:"44100 Hz · PCM 16 · 2 channels",status:"Accepted",audible:"Untested",sameContext:false,
                case:{timestamp:1793000000,error:""}}]})
    }
    AudioCheck { width: Metrics.confirmWidth; document: ({name:"Example output",canCheck:false}) }
    AudioCheck { width: Metrics.confirmWidth; document: ({name:"Example output",canCheck:true,playing:true}) }
    AudioCheck { width: Metrics.confirmWidth; document: ({name:"Example output",canCheck:true,busy:true,checking:true}) }
    Repeater {
        model: ["Busy", "Failed", "Cancelled", "Accepted"]
        AudioCheck {
            required property string modelData
            width: Metrics.confirmWidth
            document: ({name:"Example output",canCheck:true,last:{error:modelData === "Failed" ? "The output disconnected during the check." : ""},
                lastRequested:"44100 Hz · PCM 16 · 2 channels",lastOpened:modelData === "Accepted" ? "44100 Hz · PCM 16 · 2 channels" : "",
                lastStatus:modelData,lastPhysicalStatus:"Unknown",saved:modelData !== "Failed"})
        }
    }
    PillButton { objectName: "audioGalleryPreview"; text: "Preview output details"; onClicked: panel.present() }
    AudioOutputPanel {
        id: panel
        preview: true
        previewDocument: ({name:"Example output",selection:"fixture",current:"fixture",outputs:[{id:"fixture",label:"Example output",detail:""}],
            error:"The equipment changed. Read its details again.",status:"Out of date",inspection:{identity:{strong:false}},
            now:{available:false,summary:"Nothing is playing."}})
    }
}
