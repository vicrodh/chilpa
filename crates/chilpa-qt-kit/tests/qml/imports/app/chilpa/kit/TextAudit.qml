pragma Singleton
import QtQuick

// Finds text that does not fit where it is drawn: words wider than their own
// box with nothing to cut or wrap them, words that run out of a box that
// holds them, and words drawn over other words or glyphs. A test builds a
// surface in a language (I18n.choose), dresses it in the window's own face,
// optionally wider than it is (other fonts and platforms set wider), lets it
// lay out again and asks for the findings.
QtObject {
    id: audit
    // Every language the window ships, longest words first.
    readonly property var languages: ["de", "ru", "fr", "es", "pt", "nl", "ja", "en"]
    // How much wider than Inter the second pass sets every text: the
    // system's face, a Mac's or the Japanese one can run that much longer.
    readonly property real wider: 1.1
    // A data table for a TestCase: each language as Inter sets it, and wider.
    function cases() {
        const out = []
        for (let i = 0; i < languages.length; ++i) {
            out.push({ tag: languages[i], language: languages[i], scale: 1 })
            out.push({ tag: languages[i] + " wider", language: languages[i], scale: wider })
        }
        return out
    }
    // Inter, the face the window draws with unless the person picks another.
    property FontLoader face: FontLoader { source: "qrc:/qt/qml/app/chilpa/kit/assets/fonts/Inter.ttf" }
    readonly property bool ready: face.status === FontLoader.Ready
    // Tolerances, in pixels: subpixel layout and antialiasing.
    readonly property real slack: 1
    readonly property real tallSlack: 2

    function isText(item) {
        return item && "truncated" in item && "contentWidth" in item && "elide" in item && !("cursorPosition" in item)
    }
    function isInput(item) { return item && "cursorPosition" in item && "font" in item }
    // Glyphs are drawn from the icon faces' private use area; anything else
    // printable is words.
    function hasWords(item) { return /[^\s\uE000-\uF8FF\uDB80-\uDBFF\uDC00-\uDFFF]/.test(item.text) }
    function isFlickable(item) { return item && "flickableDirection" in item && "contentItem" in item }

    function walk(root, visit) {
        const stack = [root]
        while (stack.length > 0) {
            const item = stack.pop()
            if (!item) continue
            visit(item)
            const children = item.children || []
            for (let i = children.length - 1; i >= 0; --i) stack.push(children[i])
        }
    }

    // Draws every text in the window's face and, past a scale of 1, that
    // much wider: the letters are spaced out rather than enlarged, so the
    // lines keep their height and a size given in pixels stays one. A text
    // that already names a face (glyphs, the console) keeps it.
    function dress(root, scale) {
        const plain = Qt.application.font.family
        const extra = (scale === undefined ? 1 : scale) - 1
        walk(root, (item) => {
            if (!(isText(item) || isInput(item)) || item.font.family !== plain) return
            item.font.family = face.name
            // An average Latin letter is about half its size wide.
            if (extra > 0) item.font.letterSpacing = item.font.letterSpacing + item.font.pixelSize * 0.55 * extra
        })
    }

    // Where every text is, as one string: the same twice running means the
    // surface stopped moving.
    function signature(root) {
        const parts = []
        walk(root, (item) => {
            if (!isText(item)) return
            const at = item.mapToItem(root, 0, 0)
            parts.push([item.text, shown(item), Math.round(at.x), Math.round(at.y), Math.round(item.width), Math.round(item.height), Math.round(item.contentWidth)].join(","))
        })
        return parts.join(";")
    }

    // Dresses `root`, waits on `test` until nothing under it moves (rows
    // that slide in, delegates built late, layouts that answer the new face)
    // and gives the findings.
    function check(test, root, scale, options) {
        let last = ""
        for (let round = 0; round < 60; ++round) {
            dress(root, scale)
            test.wait(round < 2 ? 0 : 50)
            const now = signature(root)
            if (round >= 2 && now === last) break
            last = now
        }
        return findings(root, options)
    }

    // The part of a text's box its glyphs take, in its own coordinates.
    function painted(text) {
        const left = text.leftPadding || 0, right = text.rightPadding || 0
        const top = text.topPadding || 0, bottom = text.bottomPadding || 0
        const roomWide = text.width - left - right, roomTall = text.height - top - bottom
        const wide = text.contentWidth, tall = text.contentHeight
        const along = text.effectiveHorizontalAlignment !== undefined ? text.effectiveHorizontalAlignment : text.horizontalAlignment
        let x = left
        if (along & Qt.AlignRight) x = left + roomWide - wide
        else if (along & Qt.AlignHCenter) x = left + (roomWide - wide) / 2
        let y = top
        if (text.verticalAlignment & Qt.AlignBottom) y = top + roomTall - tall
        else if (text.verticalAlignment & Qt.AlignVCenter) y = top + (roomTall - tall) / 2
        return { x: x, y: y, width: wide, height: tall }
    }

    function shown(item) {
        let opacity = 1
        for (let at = item; at; at = at.parent) {
            if (!at.visible) return false
            opacity *= at.opacity
        }
        return opacity > 0.01
    }

    function name(item, root) {
        const names = []
        for (let at = item; at && at !== root && names.length < 3; at = at.parent)
            if (at.objectName) names.unshift(at.objectName)
        const words = !isText(item) ? String(item).replace(/\(0x[0-9a-f]+(, "[^"]*")?\)/, "") : hasWords(item) ? "\"" + (item.text.length > 48 ? item.text.slice(0, 47) + "…" : item.text) + "\"" : "a glyph"
        return words + (names.length ? " in " + names.join("/") : "")
    }

    function rectIn(item, box, other) {
        const a = item.mapToItem(other, box.x, box.y)
        const b = item.mapToItem(other, box.x + box.width, box.y + box.height)
        return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), width: Math.abs(b.x - a.x), height: Math.abs(b.y - a.y) }
    }
    function cross(a, b) {
        const x = Math.max(a.x, b.x), y = Math.max(a.y, b.y)
        return { x: x, y: y, width: Math.min(a.x + a.width, b.x + b.width) - x, height: Math.min(a.y + a.height, b.y + b.height) - y }
    }
    function contains(outer, inner) {
        for (let at = inner; at; at = at.parent) if (at === outer) return true
        return false
    }

    // What of a box the clipping items above it let through, in `root`.
    function visiblePart(item, box, root) {
        let seen = rectIn(item, box, root)
        for (let at = item.parent; at && at !== root; at = at.parent) {
            if (!at.clip) continue
            seen = cross(seen, rectIn(at, { x: 0, y: 0, width: at.width, height: at.height }, root))
            if (seen.width <= 0 || seen.height <= 0) return null
        }
        return seen
    }

    // The nearest box that holds `text` and that its glyphs run out of.
    // A scrolling view ends the climb along the way it scrolls.
    function spill(text, box, root) {
        let wide = true, tall = true
        for (let at = text.parent; at && (wide || tall); at = at.parent) {
            const view = at.parent && isFlickable(at.parent) && at === at.parent.contentItem
            if (isFlickable(at)) {
                tall = tall && at.contentHeight <= at.height + slack
                wide = wide && at.contentWidth <= at.width + slack
            }
            if (!view) {
                const r = rectIn(text, box, at)
                if (wide && at.width > 0 && (r.x < -slack || r.x + r.width > at.width + slack))
                    return { holder: at, by: Math.max(-r.x, r.x + r.width - at.width), along: "wide" }
                if (tall && at.height > 0 && (r.y < -tallSlack || r.y + r.height > at.height + tallSlack))
                    return { holder: at, by: Math.max(-r.y, r.y + r.height - at.height), along: "tall" }
            }
            if (at === root) break
        }
        return null
    }

    // The findings under `root`, one line each. `options.ignore(item)` leaves
    // an item and what it holds out on purpose.
    function findings(root, options) {
        const ignore = options && options.ignore ? options.ignore : () => false
        const out = []
        const leaves = []
        walk(root, (item) => {
            if (!isText(item) || item.text === "" || !shown(item) || ignore(item)) return
            for (let at = item.parent; at && at !== root; at = at.parent) if (ignore(at)) return
            const words = hasWords(item)
            const box = words ? painted(item) : { x: 0, y: 0, width: item.width, height: item.height }
            const seen = visiblePart(item, box, root)
            if (!seen) return
            leaves.push({ item: item, words: words, seen: seen })
            if (!words) return
            const room = item.width - (item.leftPadding || 0) - (item.rightPadding || 0)
            if (item.width > 0 && item.contentWidth > room + slack) {
                out.push(name(item, root) + " needs " + Math.ceil(item.contentWidth) + " px and has " + Math.floor(room))
                return
            }
            const ran = spill(item, box, root)
            if (ran)
                out.push(name(item, root) + " runs " + Math.ceil(ran.by) + " px out of the " + (ran.along === "wide" ? "width" : "height") + " of " + name(ran.holder, root) + " (" + Math.round(ran.holder.width) + "×" + Math.round(ran.holder.height) + ")")
        })
        // Words over words or glyphs. Lines are measured by the middle of
        // their line box, where the letters are, so stacked lines can touch.
        const core = (leaf) => {
            const r = leaf.seen, inset = r.height * (leaf.words ? 0.2 : 0.1)
            return { x: r.x, y: r.y + inset, width: r.width, height: r.height - inset * 2 }
        }
        for (let i = 0; i < leaves.length; ++i) {
            if (!leaves[i].words) continue
            const a = core(leaves[i])
            for (let j = 0; j < leaves.length; ++j) {
                if (i === j || (leaves[j].words && j < i)) continue
                if (contains(leaves[i].item, leaves[j].item) || contains(leaves[j].item, leaves[i].item)) continue
                const met = cross(a, core(leaves[j]))
                if (met.width > slack && met.height > slack)
                    out.push(name(leaves[i].item, root) + " is drawn over " + name(leaves[j].item, root) + " (" + Math.ceil(met.width) + " px)")
            }
        }
        return out
    }
}
