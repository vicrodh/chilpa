import QtQuick
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Theme"

// Focus 6, Wave Bed (03-immersive §1.4): opaque black and 48 columns of
// the left channel, every fifth sample, mirrored about the middle and
// capped at 70 % of the height, in the cover's primary color. The mapping
// runs once per publish in the binding that feeds the settle.
Rectangle {
    id: panel
    color: Theme.color.immersiveBlack
    readonly property real slot: panel.width / Stage.waveColumns
    readonly property real maxTall: panel.height * Stage.waveGain

    StreamSettle {
        id: wave
        live: panel.visible && Immersive.open
        target: {
            const s = Visualizer.waveform
            const out = new Array(Stage.waveColumns)
            for (let i = 0; i < Stage.waveColumns; ++i) {
                let v = s[i * Stage.waveStep] || 0
                if (v < 0)
                    v = -v
                out[i] = v > 1 ? 1 : v
            }
            return out
        }
    }
    Repeater {
        model: Stage.waveColumns
        Rectangle {
            required property int index
            x: index * panel.slot + Stage.waveBarLess / 2 // derived: inset in its slot
            width: panel.slot - Stage.waveBarLess
            height: Math.max(Stage.waveMinTall, wave.at(index) * panel.maxTall)
            anchors.verticalCenter: parent.verticalCenter
            radius: Stage.waveRound
            color: Ambient.primary
        }
    }
}
