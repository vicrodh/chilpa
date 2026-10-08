import QtQuick
import app.chilpa.mini
import "../Theme"

// The mini player's window (02-mini §1.1): a second top-level window in
// the same engine, frameless and transparent, with the card a gutter
// inside it so the card's edge draws over the desktop. Its size is fixed
// per surface, least and most alike, so the desktop can move it and never
// resize it: a tiling shortcut once stretched QBZ's across half a screen
// and the saved size kept it there. No parent window, or the compositor
// would tie it to the main window, which hides when this one shows.
//
// Closing it from the desktop leaves the mini player, never the app. Every
// key reaches the facade through the one host item, which has the focus
// from the start; nothing is handled here.
Window {
    id: root

    transientParent: null
    // Only where the desktop honours it: Windows. Elsewhere keeping the
    // window above others is the compositor's rule (packaging/linux).
    property bool pinnedOnTop: false
    // What the card paints behind its surfaces, from the host.
    property Component backdrop: null
    readonly property bool pinnable: Qt.platform.os === "windows"
    flags: Qt.Window | Qt.FramelessWindowHint | (pinnable && pinnedOnTop ? Qt.WindowStaysOnTopHint : 0)
    // A literal: the desktop's rules match it, so it is never translated.
    title: "Chilpa Mini Player"
    color: "transparent"

    readonly property int surfaceId: Mini.surface
    readonly property bool expanded: surfaceId >= 2
    readonly property int winWidth: Card.windowSpan
    readonly property int winHeight: expanded ? Card.heightExpanded : surfaceId === 0 ? Card.heightMicro : Card.heightCompact

    width: winWidth
    height: winHeight
    minimumWidth: winWidth
    maximumWidth: winWidth
    minimumHeight: winHeight
    maximumHeight: winHeight

    // Once the mini player has been left, a close is the app quitting,
    // which closes every window first and gives up if one refuses.
    onClosing: (close) => {
        if (!Mini.open)
            return
        close.accepted = false
        Mini.exit()
    }

    Item {
        id: keyHost
        anchors.fill: parent
        focus: true
        Component.onCompleted: keyHost.forceActiveFocus()
        Keys.onPressed: (event) => {
            const action = Mini.keyPressed(event.key, event.modifiers, event.text)
            event.accepted = action !== ""
            if (action !== "")
                card.run(action)
        }

        MiniShell {
            id: card
            x: Card.gutter
            y: Card.gutter
            width: root.width - Card.gutter * 2 // derived: a gutter on each side
            height: root.height - Card.gutter * 2 // derived: a gutter above and below
            hostWindow: root
            backdrop: root.backdrop
        }
    }
}
