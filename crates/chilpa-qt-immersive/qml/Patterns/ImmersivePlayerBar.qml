import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import app.chilpa.immersive
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Primitives"
import "../Theme"

// The floating player of the immersive view (03-immersive §1.3): a glass
// bar of min(viewport - 32, 600) by 90 with a blurred shadow; favorite,
// shuffle and repeat as ghosts at the left; times, previous, the play
// disc and next from the center; mute, the volume and the menu at the
// right; and the thin seek along the bottom. The host fades it with the
// chrome; the menu asks the host to wake.
Item {
    id: root
    property real viewportWide: Stage.barWide
    property bool shaders: true
    signal wakeAsked()
    signal exitAsked()
    implicitWidth: bar.width
    implicitHeight: bar.height

    readonly property bool playing: Player.playing && !Player.paused
    readonly property bool seekable: Player.duration > 0 && !Player.live
    readonly property real progress: Player.duration > 0 ? Player.position / Player.duration : 0

    Rectangle {
        y: Stage.barShadowDrop
        width: bar.width
        height: bar.height
        radius: Stage.barRound
        color: Theme.color.immersiveShadow
        layer.enabled: root.shaders
        layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.blurReach; blur: 0.5 }
    }
    Rectangle {
        id: bar
        width: Math.min(root.viewportWide - Stage.barLess, Stage.barWide)
        height: Stage.barTall
        radius: Stage.barRound
        color: Theme.color.immersiveGlass
        border.width: Metrics.hairline
        border.color: Theme.color.immersiveGlassEdge

        GhostButton {
            x: Stage.ghostFirst
            y: Stage.ghostTop
            glyph: Immersive.favorite ? Icons.heartFilled : Icons.heart
            filled: Immersive.favorite
            active: Immersive.favorite
            able: Immersive.markable
            onClicked: Immersive.likePlaying()
        }
        GhostButton {
            x: Stage.ghostFirst + Stage.ghostStep
            y: Stage.ghostTop
            glyph: Icons.arrowsShuffle
            active: Player.shuffle
            onClicked: Player.toggleShuffle()
        }
        GhostButton {
            x: Stage.ghostFirst + Stage.ghostStep * 2 // derived: the third ghost
            y: Stage.ghostTop
            glyph: Player.repeat === 2 ? Icons.repeatOnce : Icons.repeat
            active: Player.repeat !== 0
            onClicked: Player.cycleRepeat()
        }

        Text {
            x: bar.width / 2 - Stage.timeLeftAt // derived: left of the center
            y: Stage.volumeBarTop
            width: Stage.timeWide
            text: Player.elapsed
            color: Theme.color.immersiveTextDim
            font.pointSize: Stage.timeText
            horizontalAlignment: Text.AlignRight
        }
        NavButton {
            x: bar.width / 2 - Stage.navBackAt // derived: left of the disc
            y: Stage.ghostTop
            glyph: Icons.playerSkipBackFilled
            onClicked: Player.previous()
        }
        PlayDisc {
            objectName: "immersivePlay"
            x: bar.width / 2 - width / 2 // derived: centered
            y: (bar.height - Stage.tinyGrab - height) / 2 // derived: centered above the seek
            playing: root.playing
            shaders: root.shaders
            onClicked: Player.toggle()
        }
        NavButton {
            x: bar.width / 2 + Stage.navNextAt // derived: right of the disc
            y: Stage.ghostTop
            glyph: Icons.playerSkipForwardFilled
            onClicked: Player.next()
        }
        Text {
            x: bar.width / 2 + Stage.timeRightAt // derived: right of the center
            y: Stage.volumeBarTop
            text: Player.remaining
            color: Theme.color.immersiveTextDim
            font.pointSize: Stage.timeText
        }

        GhostButton {
            x: bar.width - Stage.volumeGhostFromRight
            y: Stage.ghostTop
            glyph: Player.muted || Player.volume === 0 ? Icons.volume3 : Icons.volume
            able: !Player.volumeLocked
            onClicked: Player.chooseMuted(!Player.muted)
        }
        VolumeBar {
            x: bar.width - Stage.volumeBarFromRight
            y: Stage.volumeBarTop
            width: Stage.volumeBarWide
            value: Player.volume / 100 // derived: a percentage to a fraction
            locked: Player.volumeLocked
            onChanged: (v) => { if (!Player.volumeLocked) Player.chooseVolume(Math.round(v * 100)) }
        }
        Rectangle {
            id: more
            objectName: "immersiveMore"
            x: bar.width - Stage.moreFromRight
            y: Stage.ghostTop
            width: Stage.ghostSide
            height: Stage.ghostSide
            radius: width / 2 // derived: a circle
            color: moreArea.containsMouse ? Theme.color.immersiveNav : "transparent"
            Text {
                anchors.centerIn: parent
                text: Icons.dots
                font.family: Icons.outline
                font.pixelSize: Stage.ghostIcon
                color: Theme.color.immersiveTextStrong
                opacity: Stage.idleGlyph
            }
            MouseArea {
                id: moreArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    root.wakeAsked()
                    exitMenu.open()
                }
            }
        }
        // A popup lives in the window's overlay, over the view: it closes
        // with the bar, or it would outlive an Escape (the owner, 2026-10-08).
        Popup {
            id: exitMenu
            objectName: "immersiveExitMenu"
            parent: more
            x: more.width - width
            y: -height - Stage.menuRowPad
            width: Stage.exitMenuWide
            padding: Stage.menuApart
            modal: false
            closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
            background: Rectangle {
                radius: Stage.menuRowRound
                color: Theme.color.immersiveMenu
                border.width: Metrics.hairline
                border.color: Theme.color.immersiveGlassEdge
            }
            // The popup sizes itself from the content's implicit height: a
            // plain height left it, and its mouse area, at zero.
            contentItem: Rectangle {
                objectName: "immersiveExitRow"
                implicitHeight: Stage.menuRowTall
                radius: Stage.menuRowRound
                color: exitArea.containsMouse ? Theme.color.immersiveRowHover : "transparent"
                Row {
                    anchors.left: parent.left
                    anchors.leftMargin: Stage.menuRowLead
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Stage.menuApart
                    Text { text: Icons.x; font.family: Icons.outline; font.pixelSize: Stage.ghostIcon; color: Theme.color.immersiveText; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: I18n.tr("Exit Immersive", I18n.rev); color: Theme.color.immersiveText; font.pointSize: Stage.menuRowText; anchors.verticalCenter: parent.verticalCenter }
                }
                MouseArea {
                    id: exitArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        exitMenu.close()
                        root.exitAsked()
                    }
                }
            }
        }

        Connections {
            target: root
            function onVisibleChanged() { if (!root.visible) exitMenu.close() }
            function onEnabledChanged() { if (!root.enabled) exitMenu.close() }
        }

        TinyBar {
            x: Stage.tinyLess
            y: Stage.tinyTop
            width: bar.width - Stage.tinyLess * 2 // derived: inset on both sides
            value: root.progress
            enabled: root.seekable
            onChanged: (v) => { if (root.seekable) Player.seek(v * Player.duration) }
        }
    }
}
