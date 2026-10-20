import QtQuick

// The translation lyrics carry, as the text for each line's place, or null
// when they carry none: the layer the lyrics name, two entries for one
// line joined by a break. The lyrics panel and the immersive view read it.
QtObject {
    // The lyrics as the player says them: {status, source, document, translation}.
    property var lyrics: null
    readonly property var lines: {
        const document = lyrics ? lyrics.document : null
        const at = lyrics ? lyrics.translation : null
        const layer = !document || at === null || at === undefined ? null : (document.layers || [])[at]
        if (!layer)
            return null
        const texts = {}
        for (const entry of layer.lines)
            texts[entry.line] = texts[entry.line] === undefined ? entry.text : texts[entry.line] + "\n" + entry.text
        return texts
    }
}
