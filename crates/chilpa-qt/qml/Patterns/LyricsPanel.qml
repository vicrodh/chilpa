import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Components" as Lines

// The lyrics of what plays. The header says the track and opens how they
// are read; the body says where the search stands or shows the lines; the
// foot says who gave them.
//
// Synced lyrics keep the line being sung in the middle and lit, with the
// lines around it fading the further they are, and a click on a line goes
// to it. The sung line is drawn word by word: timed words fill as
// they are sung, and a line timed as a whole lights up word after word.
// Every other line is one plain text, so a long song costs one item per
// line. A translation, when the lyrics carry one, is offered by a button
// over the lines and shown under each.
Item {
    id: panel

    signal closed()

    // What is said about the lyrics of what plays: nothing, that they are
    // being looked for, that there are none, or the lyrics with who gave
    // them: {status, source, document, translation}.
    readonly property var lyrics: read(Player.lyricsJson)
    readonly property bool found: lyrics !== null && !!lyrics.document
    readonly property var lines: found ? lyrics.document.lines : []
    readonly property bool wordTimed: found && (lyrics.document.timing === "word" || lyrics.document.timing === "syllable")
    readonly property string playing: Player.artist === "" || Player.title === "" ? Player.title : Player.title + " — " + Player.artist

    // The translation on offer, as the text for each line's position, or
    // null when the lyrics carry none. It is shown when asked for, and put
    // away with its track.
    readonly property var translation: found ? translated(lyrics.document.layers, lyrics.translation) : null
    property bool translating: false
    onFoundChanged: if (!found) translating = false

    // How the lines are drawn, from the options.
    readonly property real size: options.size === "s" ? Type.lyricSmall : options.size === "l" ? Type.lyricLarge
        : options.size === "xl" ? Type.lyricXLarge : Type.lyric
    readonly property color sungColor: options.color === "" ? Theme.color.textPrimary
        : options.color === "accent" ? Theme.color.accent : options.color

    function read(json) {
        if (!json)
            return null
        try {
            return JSON.parse(json)
        } catch (e) {
            return null
        }
    }

    function translated(layers, at) {
        const layer = at === null || at === undefined ? null : (layers || [])[at]
        if (!layer)
            return null
        const texts = {}
        for (const entry of layer.lines)
            texts[entry.line] = texts[entry.line] === undefined ? entry.text : texts[entry.line] + "\n" + entry.text
        return texts
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
        running: panel.visible && panel.wordTimed && !options.lite && Player.playing && !Player.paused
        onTriggered: panel.nowMs = Player.position * 1000 + Math.min(Date.now() - panel.anchoredAt, Motion.lyricCarry)
    }

    PanelHeader {
        id: head
        width: parent.width
        title: I18n.tr("Lyrics", I18n.rev)
        detail: panel.playing

        IconButton {
            objectName: "lyricsOptionsButton"
            tip: I18n.tr("Display settings", I18n.rev)
            glyph: Icons.adjustmentsHorizontal
            size: Metrics.button26
            checked: options.opened
            onClicked: options.opened ? options.close() : options.open()
        }
        IconButton {
            tip: I18n.tr("Close", I18n.rev)
            glyph: Icons.x
            size: Metrics.button26
            onClicked: panel.closed()
        }
    }

    LyricsOptions {
        id: options
        x: panel.width - width - Metrics.gap6
        y: head.height
        text: panel.found ? Player.lyrics.join("\n") : ""
        onCopied: {
            notice.text = I18n.tr("Lyrics copied", I18n.rev)
            fade.restart()
        }
    }

    Lines.LyricLines {
        id: list
        objectName: "lyricsList"
        anchors.top: head.bottom
        anchors.bottom: foot.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Metrics.gap16
        anchors.rightMargin: Metrics.gap16
        lines: panel.lines.length > 0 ? panel.lines : Player.lyrics
        synced: Player.lyricsSynced
        current: Player.lyricsSynced ? Player.lyricsLine : -1
        follow: options.follow
        nowMs: panel.nowMs
        sungEnd: panel.lineEnd(Player.lyricsLine)
        translation: panel.translating ? panel.translation : null
        size: panel.size
        sungColor: panel.sungColor
        dimming: options.dimming
        uppercase: options.uppercase
        lite: options.lite
        onSought: (ms) => Player.seek(ms / 1000)
    }

    // Over the lines, only while the lyrics carry a translation.
    IconButton {
        objectName: "translateButton"
        visible: panel.translation !== null
        anchors.right: parent.right
        anchors.bottom: foot.top
        anchors.rightMargin: Metrics.gap12
        anchors.bottomMargin: Metrics.gap8
        tip: panel.translating ? I18n.tr("Hide translation", I18n.rev) : I18n.tr("Show translation", I18n.rev)
        glyph: Icons.language
        round: true
        checked: panel.translating
        onClicked: panel.translating = !panel.translating
    }

    Text {
        objectName: "lyricsState"
        visible: !panel.found && Player.lyrics.length === 0
        anchors.centerIn: parent
        width: parent.width - Metrics.gap16 * 2 // derived: padding on both sides
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        text: panel.lyrics !== null && panel.lyrics.status === "searching" ? I18n.tr("Looking for lyrics…", I18n.rev)
            : panel.lyrics !== null || Player.playing ? I18n.tr("No lyrics for this track.", I18n.rev)
            : I18n.tr("Lyrics show here while a track plays.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textTertiary
    }

    Column {
        id: foot
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: Metrics.gap16
        anchors.rightMargin: Metrics.gap16
        topPadding: source.visible || notice.visible ? Metrics.gap6 : 0
        bottomPadding: source.visible || notice.visible ? Metrics.gap8 : 0
        spacing: Metrics.gap4

        // What was just done from the options, said for a moment.
        Text {
            id: notice
            objectName: "lyricsNotice"
            visible: text !== ""
            width: parent.width
            wrapMode: Text.Wrap
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Row {
            id: source
            objectName: "lyricsSource"
            visible: panel.found && panel.lyrics.source !== ""
            spacing: Metrics.gap4

            Text {
                text: I18n.tr("Lyrics source:", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            Text {
                objectName: "lyricsSourceName"
                text: panel.found ? panel.lyrics.source : ""
                font.pointSize: Type.small
                font.weight: Type.semibold
                color: Theme.color.accent
            }
        }
    }
    Timer {
        id: fade
        interval: Motion.toast
        onTriggered: notice.text = ""
    }
}
