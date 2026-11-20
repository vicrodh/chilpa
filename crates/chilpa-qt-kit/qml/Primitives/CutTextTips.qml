import QtQuick
import app.chilpa.kit.primitives
import "../Theme"

// Any line of text cut short says itself whole when the pointer rests on
// it, anywhere in `watching`. One watcher hanging from what holds
// everything, not a handler per line: a line under its row's hover area
// never hears the pointer itself (measured), and what holds the row always
// does. What it says is TipState's tooltip, like every other.
Item {
    id: tips

    required property Item watching
    // The line saying itself whole: kept, still shown, until the pointer leaves it when a menu hid its tooltip on purpose; let go of when it hides or another item takes the tooltip, looking again at once only while no item holds it. The one let go of when it hid is looked at again when shown.
    property Item showing: null
    property Item hid: null
    onShowingChanged: if (!showing && TipState.target === null) rest.restart()
    Connections { target: TipState; function onTargetChanged() { if (tips.showing && TipState.target !== tips.showing && (!tips.showing.visible || TipState.target)) { tips.hid = tips.showing; tips.showing = null } } }
    Connections { target: tips.showing; function onVisibleChanged() { if (tips.showing && !tips.showing.visible && TipState.target !== tips.showing) { tips.hid = tips.showing; tips.showing = null } } }
    Connections { target: tips.hid; function onVisibleChanged() { if (tips.hid && tips.hid.visible) rest.restart() } }

    width: 0; height: 0

    function under(item) {
        const at = item.mapFromItem(null, hover.position)
        return at.x >= 0 && at.y >= 0 && at.x < item.width && at.y < item.height
    }

    function letGo() {
        TipState.hide(showing)
        showing = hid = null
    }

    function look() {
        // Whatever already says itself -- a button, a card's title -- goes on.
        if (showing || TipState.target !== null || !hover.inside)
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

    // Read from the window, not from a hover handler: an area that takes
    // hover under the pointer kept it from one on what holds everything,
    // and its lines stayed quiet until a click.
    PointerWatch {
        id: hover
        onMoved: {
            if (tips.showing && !tips.under(tips.showing))
                tips.letGo()
            rest.restart()
        }
        onInsideChanged: if (!inside) tips.letGo()
    }

    Timer {
        id: rest
        interval: Metrics.cutTextLook
        onTriggered: tips.look()
    }
}
