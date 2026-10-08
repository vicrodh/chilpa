import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Components" as Lines
import "../Theme"

// Surface 4 (02-mini §1.3): the lyrics of what plays, drawn by the kit's
// lines with the panel's reading of them and the panel's options, inside
// the card's own margins and at the small size. The empty test is the
// count of lines, never a status: the mini player has no loading, error
// or offline state. Nothing here seeks: a click on a line does nothing,
// as in QBZ. The moment of the track is carried between the player's
// positions the way the panel carries it, only with word-timed lyrics,
// while it plays, and while this window is on screen.
Item {
    id: surface

    readonly property var lyrics: read(Player.lyricsJson)
    readonly property bool found: lyrics !== null && !!lyrics.document
    readonly property var lines: found ? lyrics.document.lines : []
    readonly property bool hasLines: lines.length > 0
    readonly property bool wordTimed: found && (lyrics.document.timing === "word" || lyrics.document.timing === "syllable")
    readonly property bool windowShowing: surface.Window.window
        ? (surface.Window.window.visibility !== Window.Hidden && surface.Window.window.visibility !== Window.Minimized) : true
    // The panel's options, read the way it reads them.
    readonly property bool follow: (SettingsModel.kept("ui.lyrics.follow") || "true") === "true"
    readonly property bool lite: SettingsModel.kept("ui.lyrics.lite") === "true"
    readonly property bool uppercase: SettingsModel.kept("ui.lyrics.uppercase") === "true"
    readonly property string dimming: SettingsModel.kept("ui.lyrics.dimming") || "strong"
    readonly property string tint: SettingsModel.kept("ui.lyrics.color")
    readonly property color sungColor: tint === "" ? Theme.color.textPrimary : tint === "accent" ? Theme.color.accent : tint

    function read(json) {
        if (!json)
            return null
        try {
            return JSON.parse(json)
        } catch (e) {
            return null
        }
    }

    // Where a line stops being sung when its source did not say: where the
    // next one starts, or a few seconds later for the last.
    function lineEnd(index) {
        const line = lines[index]
        if (!line)
            return 0
        if (line.end_ms !== undefined)
            return line.end_ms
        for (let i = index + 1; i < lines.length; ++i)
            if (lines[i].start_ms > line.start_ms)
                return lines[i].start_ms
        return line.start_ms + Motion.lyricLastLine
    }

    property real nowMs: Player.position * 1000
    property double anchoredAt: Date.now()
    Connections {
        target: Player
        function onPositionChanged() {
            surface.anchoredAt = Date.now()
            surface.nowMs = Player.position * 1000
        }
    }
    FrameAnimation {
        running: surface.visible && surface.windowShowing && surface.wordTimed && !surface.lite && Player.playing && !Player.paused
        onTriggered: surface.nowMs = Player.position * 1000 + Math.min(Date.now() - surface.anchoredAt, Motion.lyricCarry)
    }

    Column {
        objectName: "miniLyricsEmpty"
        anchors.centerIn: parent
        width: Math.max(0, surface.width - Card.emptyPad * 2) // derived: the padding on both sides
        visible: !surface.hasLines
        spacing: Card.emptyApart

        Item {
            width: parent.width
            height: Card.emptyIcon
            Icon {
                anchors.horizontalCenter: parent.horizontalCenter
                glyph: Icons.microphone2
                size: Card.emptyIcon
                color: Theme.color.textTertiary
            }
        }
        Text {
            width: parent.width
            text: I18n.tr("No lyrics", I18n.rev)
            color: Theme.color.textTertiary
            font.pointSize: Card.emptyText
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
    }

    Lines.LyricLines {
        anchors.fill: parent
        anchors.leftMargin: Card.lyricsPad
        anchors.rightMargin: Card.lyricsPad
        visible: surface.hasLines
        lines: surface.lines
        synced: Player.lyricsSynced
        current: Player.lyricsSynced ? Player.lyricsLine : -1
        follow: surface.follow
        nowMs: surface.nowMs
        sungEnd: surface.lineEnd(Player.lyricsLine)
        translation: null
        size: Type.lyricSmall
        sungColor: surface.sungColor
        dimming: surface.dimming
        uppercase: surface.uppercase
        lite: surface.lite
    }
}
