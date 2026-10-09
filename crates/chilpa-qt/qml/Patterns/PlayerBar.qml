import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import app.chilpa.ambient
import app.chilpa.controls
import app.chilpa.mini
import app.chilpa.immersive
import app.chilpa.waveform
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "qrc:/qt/qml/app/chilpa/controls/qml/Components" as Parts
import "qrc:/qt/qml/app/chilpa/waveform/qml/Components" as Wave

// The floating player (design/02 7): transport, the "LCD" with what plays,
// and the actions for the active output.
Item {
    id: bar

    // The queue and the lyrics are set aside by the screen (the metadata
    // editor). Their buttons then bring the panel back as it was, rather than
    // closing what is not showing.
    property bool panelAside: false
    signal showPanel()

    // Brings the set-aside panel back with `which` open.
    function reveal(which) {
        if (which === "queue")
            SettingsModel.queueOpen = true
        else
            SettingsModel.lyricsOpen = true
        showPanel()
    }

    CastPicker { id: castPicker }
    Timer { interval: Motion.refresh; repeat: true; running: true; triggeredOnStart: true; onTriggered: Cast.refresh() }
    // Standing tall with the waveform seek in a band above everything else,
    // which stays where it was (the owner, 2026-10-09).
    readonly property bool tall: Waveform.open
    implicitHeight: tall ? Metrics.playerHeightTall : Metrics.playerHeight
    Binding { target: Metrics; property: "playerSpace"; value: bar.implicitHeight }

    // The pointer is over the bar: it does not step away from under it.
    readonly property bool pointed: area.containsMouse

    // The bar floats over the content: the pointer, a click in its empty
    // space and the wheel stay here instead of reaching the list beneath.
    // no cursor: keeps the pointer and the wheel off the list beneath
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onWheel: (wheel) => wheel.accepted = true
    }

    RectangularShadow {
        anchors.fill: panel
        radius: panel.radius
        offset.y: Metrics.shadowElevatedY
        blur: Metrics.shadowElevatedBlur
        color: Theme.color.shadowElevated
    }
    RectangularShadow {
        anchors.fill: panel
        radius: panel.radius
        offset.y: Metrics.shadowKeyY
        blur: Metrics.shadowKeyBlur
        color: Theme.color.shadowElevatedKey
    }

    Rectangle {
        id: panel
        anchors.fill: parent
        radius: Metrics.radiusPlayer
        // Half alpha over the dynamic background, so the field shows through.
        color: Theme.ambient ? Theme.color.ambientPlayer : Theme.color.surfacePlayer

        Edge {}
    }

    // The bar as it always was: the band above does not move it.
    Item {
        id: body
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: Metrics.playerHeight
    }
    Wave.WaveformBand {
        objectName: "playerBand"
        visible: bar.tall
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Metrics.playerPadLeft
        anchors.rightMargin: Metrics.playerPadRight
        y: Math.round((lcd.y - height) / 2) // derived: centered between the bar's top edge and the song card
        height: Metrics.waveformBand
    }

    Row {
        id: transport
        anchors.left: parent.left
        anchors.leftMargin: Metrics.playerPadLeft
        anchors.verticalCenter: body.verticalCenter
        spacing: Metrics.gap4

        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Shuffle", I18n.rev)
            glyph: Icons.arrowsShuffle
            size: Metrics.button26
            tone: Player.shuffle ? "accent" : "secondary"
            onClicked: Player.toggleShuffle()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            visible: SettingsModel.extraControls
            tip: I18n.tr("Back 10 seconds", I18n.rev)
            glyph: SourceGlyphs.backTen
            iconFamily: SourceGlyphs.font
            size: Metrics.button26
            onClicked: Player.nudge(-10)
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Previous", I18n.rev)
            glyph: Icons.playerSkipBackFilled
            filled: true
            iconSize: Metrics.iconSkip
            tone: "primary"
            onClicked: Player.previous()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: Player.playing && !Player.paused ? I18n.tr("Pause", I18n.rev) : I18n.tr("Play", I18n.rev)
            glyph: Player.playing && !Player.paused ? Icons.playerPauseFilled : Icons.playerPlayFilled
            filled: true
            size: Metrics.button32
            iconSize: Metrics.iconSkip
            tone: "primary"
            onClicked: Player.toggle()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Next", I18n.rev)
            glyph: Icons.playerSkipForwardFilled
            filled: true
            iconSize: Metrics.iconSkip
            tone: "primary"
            onClicked: Player.next()
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            visible: SettingsModel.extraControls
            tip: I18n.tr("Forward 10 seconds", I18n.rev)
            glyph: SourceGlyphs.forwardTen
            iconFamily: SourceGlyphs.font
            size: Metrics.button26
            onClicked: Player.nudge(10)
        }
        // The A-B loop beside the transport, with the extra controls.
        Parts.LoopButton {
            anchors.verticalCenter: parent.verticalCenter
            visible: SettingsModel.extraControls && !Cast.active
        }
        IconButton {
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("Repeat", I18n.rev)
            glyph: Player.repeat === 2 ? Icons.repeatOnce : Icons.repeat
            size: Metrics.button26
            tone: Player.repeat > 0 ? "accent" : "secondary"
            onClicked: Player.cycleRepeat()
        }
    }

    Row {
        id: actions
        anchors.right: parent.right
        anchors.rightMargin: Metrics.playerPadRight
        anchors.verticalCenter: body.verticalCenter
        spacing: Metrics.gap4

        // How the window looks: today the background behind it; the other
        // views of the player join this menu as they come.
        IconButton {
            id: viewButton
            anchors.verticalCenter: parent.verticalCenter
            tip: I18n.tr("View", I18n.rev)
            glyph: Lucide.proportions
            iconFamily: Lucide.font
            // It opens a menu of several choices: lit only while that menu
            // is open, never for the choice in force.
            checked: viewMenu.opened
            onClicked: viewMenu.opened ? viewMenu.close() : viewMenu.show()

            CascadingMenu {
                id: viewMenu
                x: viewButton.width - width
                y: -height - Metrics.gap10
                readonly property var backgrounds: [I18n.tr("Off", I18n.rev), I18n.tr("Ambient", I18n.rev), I18n.tr("Blurred art", I18n.rev),
                    I18n.tr("Wallpaper", I18n.rev), I18n.tr("Wallpaper, blurred", I18n.rev)]
                function show() {
                    entries = [{ text: I18n.tr("Mini player", I18n.rev), glyph: Icons.pictureInPicture, act: "mini" },
                        { text: I18n.tr("Immersive", I18n.rev), glyph: Icons.arrowsMaximize, act: "immersive" }, { separator: true },
                        { text: I18n.tr("Background", I18n.rev), glyph: Icons.stack2, submenu: true,
                        children: backgrounds.map((text, index) => ({ text: text, checked: Ambient.mode === index, act: "background:" + index })) }]
                    open()
                }
                onActionChosen: (entry) => {
                    if (entry.act === "mini")
                        Mini.enter()
                    else if (entry.act === "immersive")
                        Immersive.openFromMenu()
                    else if (entry.act.startsWith("background:"))
                        Ambient.choose(Number(entry.act.split(":")[1]))
                }
            }
        }
        VolumeControl {
            anchors.verticalCenter: parent.verticalCenter
            visible: bar.width >= Metrics.playerNoVolume
        }
        IconButton {
            id: optionsButton
            tip: I18n.tr("Playback options", I18n.rev)
            glyph: Icons.adjustmentsHorizontal
            // Lit when something differs from how it came.
            checked: options.opened || !Player.gapless || (Player.normalize && Player.normalizeApplies) || Player.automix
            onClicked: options.opened ? options.close() : options.open()

            PlaybackOptions {
                id: options
                x: optionsButton.width - width
                y: -height - Metrics.gap10
            }
        }
        IconButton {
            tip: Cast.active ? Cast.destination : I18n.tr("Play on", I18n.rev)
            glyph: Cast.active && lcd.castState.icon === "dlna" ? SourceGlyphs.dlna : SourceGlyphs.castAudio
            iconFamily: SourceGlyphs.font
            tone: Cast.active ? "accent" : "secondary"
            checked: Cast.active || castPicker.opened
            onClicked: castPicker.show(this)
        }
        IconButton {
            tip: I18n.tr("Lyrics", I18n.rev)
            glyph: Icons.quote
            checked: SettingsModel.lyricsOpen && !bar.panelAside
            onClicked: bar.panelAside ? bar.reveal("lyrics") : SettingsModel.lyricsOpen = !SettingsModel.lyricsOpen
        }
        IconButton {
            tip: I18n.tr("Queue", I18n.rev)
            glyph: Icons.listDetails
            checked: SettingsModel.queueOpen && !bar.panelAside
            onClicked: bar.panelAside ? bar.reveal("queue") : SettingsModel.queueOpen = !SettingsModel.queueOpen
        }
        // The caret: up, the bar stands tall with the waveform seek.
        IconButton {
            objectName: "playerUnfold"
            tip: Waveform.open ? I18n.tr("Plain seek bar", I18n.rev) : I18n.tr("Waveform", I18n.rev)
            glyph: Waveform.open ? Icons.chevronDown : Icons.chevronUp
            onClicked: Waveform.open = !Waveform.open
        }
    }

    PlayerLcd {
        id: lcd
        tall: bar.tall
        anchors.left: transport.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: actions.left
        anchors.rightMargin: Metrics.gap10
        anchors.verticalCenter: body.verticalCenter
        showTimes: bar.width >= Metrics.playerNoTimes
    }

    QualityTooltip {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.top
        anchors.bottomMargin: Metrics.gap10
        visible: lcd.qualityHovered && (Player.playing || Cast.active)
        z: Metrics.tooltipLayer
    }
}
