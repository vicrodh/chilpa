import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Components" as Lines
import "qrc:/qt/qml/app/chilpa/lyrics/qml/Primitives" as LyricData

// I center the sung line with timed word fill, optional translation,
// source credit and display controls. Clicking a timed line seeks to it.
Item {
    id: panel

    signal closed()
    // Under the queue's art it is a tab: nothing to close, no header (the
    // tabs hold its settings), lines from edge to edge, a size of its own.
    property bool closable: true
    readonly property real inset: closable ? Metrics.gap16 : 0
    readonly property bool optionsOpen: options.opened
    function toggleOptions() { options.opened ? options.close() : options.open() }

    // What is said about the lyrics of what plays: nothing, that they are
    // being looked for, that there are none, or the lyrics with who gave
    // them: {status, source, document, translation}.
    readonly property var lyrics: read(Player.lyricsJson)
    readonly property bool found: lyrics !== null && !!lyrics.document
    readonly property var lines: found ? lyrics.document.lines : []
    readonly property bool waitingForLyrics: Player.lyricsSynced && options.follow && lines.length > 0
        && lines[0].start_ms !== undefined && nowMs < lines[0].start_ms
    readonly property bool wordTimed: found && (lyrics.document.timing === "word" || lyrics.document.timing === "syllable")
    readonly property string playing: Player.artist === "" || Player.title === "" ? Player.title : Player.title + " — " + Player.artist

    // The translation on offer, as the text for each line's position, or
    // null when the lyrics carry none. It is shown when asked for, and put
    // away with its track.
    readonly property var translation: found ? offered.lines : null
    LyricData.LyricTranslation { id: offered; lyrics: panel.lyrics }
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

    // I carry the position between transport updates on the shared pulse.
    property real nowMs: Player.position * 1000
    property double anchoredAt: Date.now()
    Connections {
        target: Player
        function onPositionChanged() {
            panel.anchoredAt = Date.now()
            panel.nowMs = Player.position * 1000
        }
    }
    PulseClock {
        running: panel.visible && (!panel.Window.window || panel.Window.window.visible && panel.Window.window.visibility !== Window.Minimized)
            && panel.wordTimed && !options.lite && Player.playing && !Player.paused
        onTriggered: panel.nowMs = Player.position * 1000 + Math.min(Date.now() - panel.anchoredAt, Motion.lyricCarry)
    }

    PanelHeader {
        id: head
        visible: panel.closable
        width: parent.width
        height: visible ? implicitHeight : 0
        title: I18n.tr("Lyrics", I18n.rev)
        detail: panel.playing

        IconButton {
            objectName: "lyricsOptionsButton"
            tip: I18n.tr("Display settings", I18n.rev)
            glyph: Icons.adjustmentsHorizontal
            size: Metrics.button26
            checked: options.opened
            onClicked: panel.toggleOptions()
        }
        IconButton {
            visible: panel.closable
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
        sizeKey: panel.closable ? "size" : "size.queue"
        sizeFirst: panel.closable ? "m" : "s"
        text: panel.found ? Player.lyrics.join("\n") : ""
        onCopied: {
            notice.text = I18n.tr("Lyrics copied", I18n.rev)
            fade.restart()
        }
    }

    Lines.LyricLines {
        id: list
        objectName: "lyricsList"
        visible: !panel.waitingForLyrics
        anchors.top: head.bottom
        anchors.bottom: foot.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: panel.inset
        anchors.rightMargin: panel.inset
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
        onFollowAsked: on => options.choose("follow", on)
    }

    Image {
        objectName: "lyricsPrelude"
        anchors.centerIn: list
        width: Math.min(list.width, list.height) / 3 // derived: a quiet mark inside the lyric viewport
        height: width
        source: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"
        fillMode: Image.PreserveAspectFit
        opacity: 0.25 // requested: one quarter visible
        visible: panel.waitingForLyrics
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
            : panel.lyrics !== null && panel.lyrics.status === "failed" ? I18n.tr("Lyrics could not be retrieved. Try again shortly.", I18n.rev)
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
        anchors.leftMargin: panel.inset
        anchors.rightMargin: panel.inset
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
