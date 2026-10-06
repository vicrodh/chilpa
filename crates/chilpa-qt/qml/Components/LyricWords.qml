import QtQuick
import "../Theme"

// One line of lyrics, word by word, while it is being sung.
//
// Timed words fill with the lead colour as they are sung, each in its own
// time and in proportion to its letters, so a line that wraps fills in
// reading order without measuring where it breaks. A line timed only as a
// whole lights up word after word instead: it does not pretend to know
// when each word is sung.
Flow {
    id: words

    // The line as the lyrics document has it: text, start_ms, end_ms and,
    // when timed by word, segments.
    property var line: ({})
    // Where the line ends when its source did not say.
    property real endMs: 0
    // The moment of the track, in milliseconds.
    property real nowMs: 0
    property alias font: metrics.font
    // The color of what has been sung, and of what has not yet.
    property color sungColor: Theme.color.textPrimary
    property color restColor: Theme.color.textTertiary
    // The room a space takes in this font, between two words.
    readonly property real space: metrics.advanceWidth(" ")

    readonly property bool timed: (line.segments || []).some(s => !s.background)
    readonly property var groups: timed ? grouped(line.segments, endMs) : untimed(line.text || "")

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

    FontMetrics {
        id: metrics
        font.pointSize: Type.lyric
        font.weight: Type.bold
    }

    Repeater {
        model: words.groups

        delegate: Item {
            id: word
            required property int index
            required property var modelData
            // How lit a word of a line timed as a whole is.
            property real lit: words.timed ? 1 : 0

            width: base.implicitWidth + (modelData.space ? words.space : 0)
            height: base.implicitHeight

            Text {
                id: base
                text: word.modelData.text
                font: metrics.font
                color: words.restColor
            }
            Item {
                clip: true
                width: words.timed ? base.implicitWidth * words.progress(word.modelData, words.nowMs) : base.implicitWidth
                height: base.height
                opacity: word.lit
                Text {
                    text: word.modelData.text
                    font: metrics.font
                    color: words.sungColor
                }
            }

            SequentialAnimation on lit {
                running: !words.timed
                PauseAnimation { duration: word.index * Motion.lyricStagger }
                NumberAnimation { to: 1; duration: Motion.base }
            }
        }
    }
}
