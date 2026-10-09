import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// One line of lyrics, word by word, while it is being sung.
//
// The line is written once, as one text wrapped the way every other line
// is, and what has been sung is laid over it word by word. So a line breaks
// in the same places sung or not: lighting it up never moves a word to the
// line above or below.
//
// Timed words fill with the lead colour as they are sung, each in its own
// time and in proportion to its letters. A line timed only as a whole
// lights up word after word instead: it does not pretend to know when each
// word is sung.
Item {
    id: words

    // The line as the lyrics document has it: text, start_ms, end_ms and,
    // when timed by word, segments.
    property var line: ({})
    // Where the line ends when its source did not say.
    property real endMs: 0
    // The moment of the track, in milliseconds.
    property real nowMs: 0
    property alias font: base.font
    // The color of what has been sung, and of what has not yet.
    property color sungColor: Theme.color.textPrimary
    property color restColor: Theme.color.textTertiary
    // Centered words and a raised shadow under them, for a line alone
    // over a cover.
    property bool centered: false
    property color shadowColor: "transparent"

    readonly property bool timed: (line.segments || []).some(s => !s.background)
    readonly property var groups: timed ? grouped(line.segments, endMs) : untimed(line.text || "")
    // The words as one text, and where each one starts in it.
    readonly property string text: timed ? groups.map(g => g.text + (g.space ? " " : "")).join("").trim() : (line.text || "")
    readonly property var starts: placed(groups, text)

    implicitHeight: base.implicitHeight

    // The lead segments joined into words, each with when every piece ends.
    function grouped(segments, lineEnd) {
        const lead = segments.filter(s => !s.background)
        const out = []
        let current = null
        for (let i = 0; i < lead.length; ++i) {
            const s = lead[i]
            const next = i + 1 < lead.length ? lead[i + 1].start_ms : lineEnd
            const piece = { text: s.text, start: s.start_ms, end: Math.max(s.start_ms, s.end_ms !== undefined ? s.end_ms : next) }
            if (!current)
                current = { text: "", pieces: [], space: false }
            current.text += s.text
            current.pieces.push(piece)
            if (s.space_after || i + 1 === lead.length) {
                current.space = s.space_after
                out.push(current)
                current = null
            }
        }
        return out
    }

    function untimed(text) {
        return text.split(" ").filter(w => w.length > 0).map(w => ({ text: w, pieces: [], space: true }))
    }

    function placed(groups, text) {
        let from = 0
        return groups.map(g => {
            const at = text.indexOf(g.text, from)
            from = at < 0 ? from : at + g.text.length
            return at
        })
    }

    // How much of a word has been sung, from 0 to 1, weighed by letters.
    function progress(group, now) {
        let total = 0, sung = 0
        for (const p of group.pieces)
            total += p.text.length
        for (const p of group.pieces) {
            if (now >= p.end) {
                sung += p.text.length
                continue
            }
            if (now > p.start && p.end > p.start)
                sung += p.text.length * (now - p.start) / (p.end - p.start)
            break
        }
        return total > 0 ? sung / total : 0
    }

    // Where a word sits in the wrapped line: its corner, in this item.
    function spot(index) {
        const at = starts[index]
        // Until the text is laid out there is nowhere to be.
        return at >= 0 && at <= measure.length ? measure.positionToRectangle(at) : Qt.rect(0, 0, 0, 0)
    }

    Text {
        id: base
        width: words.width
        text: words.text
        wrapMode: Text.Wrap
        horizontalAlignment: words.centered ? Text.AlignHCenter : Text.AlignLeft
        font.pointSize: Type.lyric
        font.weight: Type.bold
        color: words.restColor
        style: words.shadowColor.a > 0 ? Text.Raised : Text.Normal
        styleColor: words.shadowColor
    }
    // A Text does not say where its words ended up; a text field laid out
    // the same way does. It is never drawn.
    TextEdit {
        id: measure
        visible: false
        enabled: false
        readOnly: true
        width: words.width
        text: words.text
        textFormat: TextEdit.PlainText
        wrapMode: TextEdit.Wrap
        horizontalAlignment: words.centered ? TextEdit.AlignHCenter : TextEdit.AlignLeft
        font: base.font
    }

    Repeater {
        model: words.groups

        delegate: Item {
            id: word
            required property int index
            required property var modelData
            // Asked again whenever the line is laid out anew.
            readonly property rect at: (measure.contentWidth, measure.contentHeight, words.spot(index))
            // How lit a word of a line timed as a whole is.
            property real lit: words.timed ? 1 : 0

            visible: words.starts[index] >= 0
            x: at.x
            y: at.y
            width: words.timed ? sung.implicitWidth * words.progress(modelData, words.nowMs) : sung.implicitWidth
            height: sung.implicitHeight
            clip: true
            opacity: lit

            Text {
                id: sung
                text: word.modelData.text
                font: base.font
                color: words.sungColor
            }

            SequentialAnimation on lit {
                running: !words.timed
                PauseAnimation { duration: word.index * Motion.lyricStagger }
                NumberAnimation { to: 1; duration: Motion.base }
            }
        }
    }
}
