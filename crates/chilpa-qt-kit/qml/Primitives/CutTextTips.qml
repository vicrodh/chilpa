import QtQuick
import "../Theme"

// Any line of text cut short says itself whole when the pointer rests on
// it, anywhere in `watching`. One watcher hanging from what holds
// everything, not a handler per line: a line under its row's hover area
// never hears the pointer itself (measured), and what holds the row always
// does. What it says is TipState's tooltip, like every other.
Item {
    id: tips

    required property Item watching
    // The line saying itself whole, while the pointer is on it.
    property Item showing: null

    width: 0
    height: 0

    function under(item) {
        const at = item.mapFromItem(tips.watching, hover.point.position)
        return at.x >= 0 && at.y >= 0 && at.x < item.width && at.y < item.height
    }

    function letGo() {
        if (showing)
            TipState.hide(showing)
        showing = null
    }

    function look() {
        // Whatever already says itself -- a button, a card's title -- goes on.
        if (showing || TipState.target !== null || !hover.hovered)
            return
        const cut = cutAt(tips.watching)
        if (cut) {
            showing = cut
            TipState.show(cut.text, cut)
        }
    }

    // The topmost line under the pointer that is cut short. What does not
    // hold the pointer is not looked into, unless it has no size of its own
    // and places what it holds around itself.
    function cutAt(item) {
        for (let i = item.children.length - 1; i >= 0; i--) {
            const child = item.children[i]
            if (!child.visible || child.opacity === 0)
                continue
            const inside = under(child)
            if (!inside && (child.clip || (child.width > 0 && child.height > 0)))
                continue
            const found = cutAt(child)
            if (found)
                return found
            if (inside && child.truncated === true)
                return child
        }
        return null
    }

    HoverHandler {
        id: hover
        parent: tips.watching
        onPointChanged: {
            if (tips.showing && !tips.under(tips.showing))
                tips.letGo()
            rest.restart()
        }
        onHoveredChanged: if (!hovered) tips.letGo()
    }

    Timer {
        id: rest
        interval: Metrics.cutTextLook
        onTriggered: tips.look()
    }
}
