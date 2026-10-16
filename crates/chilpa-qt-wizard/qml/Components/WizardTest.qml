import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.wizard
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// Step 4: the four tracks (a click jumps to one while the test plays),
// Play test or Stop, Previous and Next on whatever plays, and "Use my
// current queue"; under them what Chilpa asked the device for against what
// the card runs at, with a check when the rate is the same. The card is
// asked every second and a half while the test plays and the step shows.
Column {
    id: step
    property var doc: ({})
    readonly property bool playing: doc.testPlaying === true
    spacing: Metrics.gap14

    Text {
        width: parent.width
        text: I18n.tr("Test Playback", I18n.rev)
        color: Theme.color.textPrimary
        font.pointSize: Type.section
        font.weight: Type.semibold
    }
    Text {
        width: parent.width
        text: I18n.tr("Plays four tracks at different bit-depths and sample rates through the selected DAC. Watch your DAC's display switch rates — and confirm it below.", I18n.rev)
        color: Theme.color.textSecondary
        font.pointSize: Type.body
        wrapMode: Text.WordWrap
    }
    Column {
        width: parent.width
        spacing: Metrics.gap4
        Repeater {
            model: step.doc.testTracks || []
            delegate: Rectangle {
                id: track
                required property int index
                required property string modelData
                objectName: "wizardTrack" + index
                width: step.width
                height: WizardFrame.trackRow
                radius: Metrics.radiusItem
                color: hover.hovered && step.playing ? Theme.color.stateHover : "transparent"
                Text {
                    id: glyph
                    anchors.left: parent.left
                    anchors.leftMargin: Metrics.gap6
                    width: WizardFrame.trackGlyph
                    height: parent.height
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    text: hover.hovered && step.playing ? "▶" : "♪"
                    color: hover.hovered && step.playing ? Theme.color.accent : Theme.color.textTertiary
                    font.pointSize: Type.small
                }
                Text {
                    anchors.left: glyph.right
                    anchors.leftMargin: Metrics.gap8
                    anchors.right: parent.right
                    anchors.rightMargin: Metrics.gap6
                    anchors.verticalCenter: parent.verticalCenter
                    text: track.modelData
                    color: Theme.color.textSecondary
                    font.pointSize: Type.small
                    elide: Text.ElideRight
                }
                HoverHandler { id: hover; cursorShape: step.playing ? Qt.PointingHandCursor : Qt.ArrowCursor }
                TapHandler { enabled: step.playing; onTapped: HifiWizard.command("testPlayIndex", String(track.index)) }
            }
        }
    }
    Row {
        spacing: Metrics.gap10
        PillButton {
            objectName: "wizardPlay"
            kind: "primary"
            text: step.playing ? I18n.tr("Stop", I18n.rev) : I18n.tr("Play test", I18n.rev)
            onClicked: HifiWizard.command(step.playing ? "stopTest" : "startTest", "")
        }
        IconButton {
            visible: step.playing
            size: Metrics.button32
            glyph: Lucide.skipBack
            iconFamily: Lucide.font
            tip: I18n.tr("Previous", I18n.rev)
            onClicked: Player.previous()
        }
        IconButton {
            visible: step.playing
            size: Metrics.button32
            glyph: Lucide.skipForward
            iconFamily: Lucide.font
            tip: I18n.tr("Next", I18n.rev)
            onClicked: Player.next()
        }
        PillButton {
            objectName: "wizardOwnQueue"
            visible: !step.playing
            text: I18n.tr("Use my current queue", I18n.rev)
            onClicked: HifiWizard.command("verifyOwn", "")
        }
    }
    Rectangle {
        objectName: "wizardReadback"
        visible: step.playing || (step.doc.testRequestedLabel || "") !== ""
        width: parent.width
        height: readout.implicitHeight + Metrics.gap24
        radius: Metrics.radiusControl
        color: Theme.color.surfaceRaised
        border.width: 1
        border.color: Theme.color.strokeHairline
        Column {
            id: readout
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Metrics.gap12
            spacing: Metrics.gap6
            Text {
                width: parent.width
                text: step.doc.testRequestedLabel || ""
                color: Theme.color.textPrimary
                font.pointSize: Type.body
                font.weight: Type.medium
                wrapMode: Text.WordWrap
            }
            Row {
                spacing: Metrics.gap8
                Text {
                    objectName: "wizardMatched"
                    visible: step.doc.testRateMatched === true
                    text: "✓"
                    color: Theme.color.statusPositive
                    font.pointSize: Type.body
                    font.weight: Type.bold
                }
                Text {
                    text: step.doc.testNegotiatedLabel || ""
                    color: step.doc.testRateMatched === true ? Theme.color.statusPositive : Theme.color.textSecondary
                    font.pointSize: Type.body
                }
            }
        }
    }
    Timer {
        interval: WizardFrame.pollMs
        repeat: true
        running: step.visible && step.playing
        onTriggered: HifiWizard.command("pollTest", "")
    }
}
