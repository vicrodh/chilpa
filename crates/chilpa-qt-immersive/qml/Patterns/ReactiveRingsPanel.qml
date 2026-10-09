import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Focus 9, Reactive Rings (03-immersive §1.4): five concentric rings
// breathe with the five energy bands, outer sub-bass to inner air, over
// four discs of bloom; onsets launch rings from a pool of twelve that
// expand and fade on the pulse. The pack is stashed on publish and read
// on the pulse; the palette is the cover's six colors, walked by a phase
// that moves only while real audio does.
Rectangle {
    id: panel
    color: Theme.color.immersiveBlack
    clip: true
    readonly property bool windowShowing: panel.Window.window
        ? (panel.Window.window.visibility !== Window.Minimized && panel.Window.window.visibility !== Window.Hidden) : true
    readonly property bool active: visible && Immersive.open && windowShowing
    readonly property real minDim: Math.min(width, height)

    property var bandEnergy: [0, 0, 0, 0, 0]
    property real globalEnergy: 0
    property real colorPhase: 0
    property real previousBass: 0
    property real previousHit: 0
    property int hitCooldown: 0
    property var pulseRings: []
    property int nextColor: 0
    property var pendingPack: null

    // The cover's six colors, run into a ramp of 48 by the facade: a
    // position on the ring picks one, and the alpha goes on the item.
    readonly property var ramp: {
        try {
            const list = JSON.parse(Ambient.ringRampJson)
            if (list.length > 0)
                return list
        } catch (e) {
        }
        return [Ambient.primary, Ambient.accent, Ambient.secondary]
    }
    readonly property var bandNames: ["SUB", "BASS", "MID", "PRES", "AIR"]
    readonly property int paletteCount: 6

    function rampColor(position) {
        const count = ramp.length
        let wrapped = position % paletteCount
        if (wrapped < 0)
            wrapped += paletteCount
        return ramp[Math.floor(wrapped / paletteCount * count) % count]
    }
    function stash() {
        try {
            pendingPack = JSON.parse(Visualizer.packJson)
        } catch (e) {
        }
    }
    onVisibleChanged: if (visible) stash()
    Component.onCompleted: stash()
    Connections { target: Visualizer; function onPackJsonChanged() { panel.stash() } }
    Connections {
        target: Player
        function onPathChanged() {
            // A new palette starts from a clean pulse history.
            panel.previousBass = 0
            panel.previousHit = 0
            panel.hitCooldown = 0
            panel.pulseRings = []
        }
    }

    property bool holding: false
    function sync() {
        if (active === holding)
            return
        holding = active
        if (holding)
            Pulse.hold()
        else
            Pulse.release()
    }
    onActiveChanged: sync()
    Component.onDestruction: if (holding) Pulse.release()

    Connections {
        target: Pulse
        function onMsChanged() {
            if (!panel.active)
                return
            const pack = panel.pendingPack
            let spawn = false
            let intensity = 0
            if (pack !== null) {
                panel.pendingPack = null
                const lo = pack.energyLo || [0, 0, 0, 0]
                const hi = pack.energyHi || [0, 0, 0, 0]
                const energy = [lo[0] || 0, lo[1] || 0, lo[2] || 0, lo[3] || 0, hi[0] || 0]
                panel.bandEnergy = energy
                panel.globalEnergy = (energy[0] + energy[1] + energy[2] + energy[3] + energy[4]) / 5
                const bass = (energy[0] * 2 + energy[1] * 1.5) / 3.5
                const bassDelta = bass - panel.previousBass
                panel.previousBass = bass
                const beatAbove = pack.beatAbove || 0
                const transientAmp = pack.transient || 0
                const hit = Math.max(beatAbove, transientAmp * 0.78)
                if (panel.hitCooldown > 0)
                    panel.hitCooldown--
                if (panel.hitCooldown <= 0 && (bassDelta > 0.045 || (hit > 0.16 && hit - panel.previousHit > 0.035))) {
                    spawn = true
                    intensity = Math.min(1, Math.max(bassDelta * 4.2, hit))
                    panel.hitCooldown = Stage.ringsCooldown
                }
                panel.previousHit = hit
                if (Player.playing && !Player.paused && panel.globalEnergy > 0.006)
                    panel.colorPhase = (panel.colorPhase + 0.008 + panel.globalEnergy * 0.035 + beatAbove * 0.055) % 6
            }
            const old = panel.pulseRings
            const next = []
            for (let i = 0; i < old.length; ++i) {
                const ring = old[i]
                const reach = ring.reach + ring.speed
                const alpha = ring.alpha * 0.94
                if (alpha >= 0.012 && reach <= ring.limit * 1.35)
                    next.push({ reach: reach, limit: ring.limit, alpha: alpha, stroke: ring.stroke, speed: ring.speed, color: ring.color })
            }
            if (spawn && Player.playing && !Player.paused) {
                next.push({ reach: 0.11, limit: 0.34 + intensity * 0.23, alpha: 0.58 + intensity * 0.40, stroke: Stage.ringsStroke + intensity * Stage.ringsStrokeGain, speed: 0.006 + intensity * 0.013, color: panel.nextColor })
                panel.nextColor = (panel.nextColor + 1) % panel.paletteCount
                if (next.length > Stage.ringsPool)
                    next.shift()
            }
            if (next.length > 0 || old.length > 0 || spawn)
                panel.pulseRings = next
        }
    }

    Repeater {
        model: 4
        Rectangle {
            required property int index
            readonly property real energy: panel.globalEnergy
            readonly property real disc: panel.minDim * (0.10 + (3 - index) * 0.047 + energy * 0.035)
            width: disc
            height: disc
            x: (panel.width - width) / 2 // derived: centered
            y: (panel.height - height) / 2 // derived: centered
            radius: width / 2 // derived: a disc
            color: panel.rampColor(index * 0.7 + panel.colorPhase)
            opacity: Math.min(1, (0.018 + energy * 0.045) * (index + 1))
        }
    }
    Repeater {
        model: Stage.ringsPool
        Item {
            id: pulseRing
            required property int index
            readonly property bool populated: index < panel.pulseRings.length
            readonly property var ring: populated ? panel.pulseRings[index] : null
            readonly property real diameter: populated ? ring.reach * panel.minDim * 2 : 0
            visible: populated
            x: (panel.width - diameter) / 2 // derived: centered
            y: (panel.height - diameter) / 2 // derived: centered
            width: diameter
            height: diameter
            Rectangle {
                anchors.fill: parent
                anchors.margins: -Stage.eqBar
                radius: width / 2 // derived: a ring
                color: "transparent"
                border.width: pulseRing.ring ? pulseRing.ring.stroke + Stage.metaApart : 0
                border.color: pulseRing.ring ? panel.rampColor(pulseRing.ring.color + panel.colorPhase) : "transparent"
                opacity: pulseRing.ring ? pulseRing.ring.alpha * 0.20 : 0
            }
            Rectangle {
                anchors.fill: parent
                radius: width / 2 // derived: a ring
                color: "transparent"
                border.width: pulseRing.ring ? Math.max(1, pulseRing.ring.stroke) : 0
                border.color: pulseRing.ring ? panel.rampColor(pulseRing.ring.color + panel.colorPhase) : "transparent"
                opacity: pulseRing.ring ? pulseRing.ring.alpha : 0
            }
        }
    }
    Repeater {
        model: 5
        Item {
            id: band
            required property int index
            readonly property real energy: panel.bandEnergy[index] || 0
            readonly property real radiusShare: 0.115 + (4 - index) * 0.066 + energy * 0.058
            readonly property real diameter: radiusShare * panel.minDim * 2
            x: (panel.width - diameter) / 2 // derived: centered
            y: (panel.height - diameter) / 2 // derived: centered
            width: diameter
            height: diameter
            Rectangle {
                anchors.fill: parent
                anchors.margins: -(Stage.eqBar + band.energy * Stage.eqStep)
                radius: width / 2 // derived: a ring
                color: "transparent"
                border.width: Stage.ringsHalo + band.energy * Stage.ringsHaloGain
                border.color: panel.rampColor(index + panel.colorPhase)
                opacity: 0.055 + band.energy * 0.18
            }
            Rectangle {
                anchors.fill: parent
                radius: width / 2 // derived: a ring
                color: "transparent"
                border.width: Stage.ringsCore + band.energy * Stage.ringsCoreGain
                border.color: panel.rampColor(index + panel.colorPhase)
                opacity: Math.min(1, 0.20 + band.energy * 0.72)
            }
        }
    }
    Row {
        visible: panel.height >= Stage.ringsLabelMinTall
        anchors.horizontalCenter: parent.horizontalCenter
        y: panel.height / 2 + panel.minDim * Stage.ringsLabelAt // derived: below the rings
        Repeater {
            model: 5
            Text {
                required property int index
                width: Stage.ringsLabelWide
                text: panel.bandNames[index]
                horizontalAlignment: Text.AlignHCenter
                color: panel.rampColor(index + panel.colorPhase)
                opacity: Math.min(1, 0.32 + (panel.bandEnergy[index] || 0) * 0.55)
                font.pointSize: Stage.ringsLabelText
                font.family: "monospace"
                font.letterSpacing: Stage.ringsLabelTracking
            }
        }
    }
}
