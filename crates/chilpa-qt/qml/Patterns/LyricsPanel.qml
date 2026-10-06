import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The lyrics of what plays. Synced lyrics keep the line being sung in the
// middle and lit; unsynced ones are simply text to read.
//
// The line being sung is drawn word by word: timed words fill as they are
// sung, and a line timed as a whole lights up word after word. Every other
// line is one plain text, so a long song costs one item per line.
Item {
    id: panel

    signal closed()

    // The whole lyrics with where they came from: {source, document}.
    readonly property var lyrics: read(Player.lyricsJson)
    readonly property var lines: lyrics ? lyrics.document.lines : []
    readonly property bool wordTimed: lyrics !== null && (lyrics.document.timing === "word" || lyrics.document.timing === "syllable")

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

    // The moment of the track. The position arrives four times a second; in
    // between it is carried forward by the frame clock, never further than
    // the next arrival could correct.
    property real nowMs: Player.position * 1000
    property double anchoredAt: Date.now()
    Connections {
        target: Player
        function onPositionChanged() {
            panel.anchoredAt = Date.now()
            panel.nowMs = Player.position * 1000
        }
    }
    FrameAnimation {
        running: panel.visible && panel.wordTimed && Player.playing && !Player.paused
        onTriggered: panel.nowMs = Player.position * 1000 + Math.min(Date.now() - panel.anchoredAt, Motion.lyricCarry)
    }

    PanelHeader {
        id: head
        width: parent.width
        title: I18n.tr("Lyrics", I18n.rev)

        IconButton {
            tip: I18n.tr("Close", I18n.rev)
            glyph: Icons.x
            size: Metrics.button26
            onClicked: panel.closed()
        }
    }

    ListView {
        id: list
        objectName: "lyricsList"
        anchors.top: head.bottom
        anchors.bottom: parent.bottom
        anchors.leftMargin: Metrics.gap16
        anchors.rightMargin: Metrics.gap16
        anchors.left: parent.left
        anchors.right: parent.right
        clip: true
        model: panel.lines.length > 0 ? panel.lines : Player.lyrics
        spacing: Metrics.gap8
        currentIndex: Player.lyricsSynced ? Player.lyricsLine : -1
        highlightRangeMode: Player.lyricsSynced ? ListView.ApplyRange : ListView.NoHighlightRange
        preferredHighlightBegin: height / 3 // derived: the sung line a third of the way down
        preferredHighlightEnd: height / 3 + Metrics.shelfHeader // derived: one line's room
        highlightMoveDuration: Motion.panel
        boundsBehavior: Flickable.StopAtBounds

        delegate: Loader {
            id: row
            required property int index
            required property var modelData
            readonly property string text: typeof modelData === "string" ? modelData : modelData.text
            readonly property bool sung: Player.lyricsSynced && index === Player.lyricsLine
            width: ListView.view.width
            sourceComponent: sung && typeof modelData !== "string" ? wordsRow : plainRow

            Component {
                id: plainRow
                Text {
                    text: row.text
                    wrapMode: Text.Wrap
                    font.pointSize: Type.lyric
                    font.weight: Type.bold
                    color: !Player.lyricsSynced || row.sung ? Theme.color.textPrimary : Theme.color.textTertiary
                }
            }
            Component {
                id: wordsRow
                LyricWords {
                    objectName: "sungLine"
                    line: row.modelData
                    endMs: panel.lineEnd(row.index)
                    nowMs: panel.nowMs
                }
            }
        }

        footer: Text {
            visible: panel.lyrics !== null && panel.lyrics.source !== ""
            width: ListView.view ? ListView.view.width : 0
            topPadding: Metrics.gap16
            wrapMode: Text.Wrap
            text: panel.lyrics ? I18n.trArgs("Lyrics from {}", [panel.lyrics.source], I18n.rev) : ""
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
    }

    Text {
        visible: Player.lyrics.length === 0
        anchors.centerIn: parent
        width: parent.width - Metrics.gap16 * 2 // derived: padding on both sides
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: Player.playing ? I18n.tr("No lyrics for this track.", I18n.rev) : I18n.tr("Lyrics show here while a track plays.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }
}
