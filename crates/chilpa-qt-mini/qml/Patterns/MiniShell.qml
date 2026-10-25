import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.kit
import app.chilpa.mini
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Theme"

// The mini player's card (02-mini §1.2): the window less its gutter. It
// owns the card's chrome, the backdrop behind every surface, the router
// of the five surfaces and the footer, and it runs the actions a key in
// its window comes to. The card inherits the theme and the dynamic
// background of the app; its own button can put the blurred cover behind
// it instead, and turning that off gives the app's background back.
//
// A press on the free area hands the window to the desktop to move; the
// queue and the lyrics keep their content area for scrolling.
Rectangle {
    id: card

    // The mini player's window, handed down for the drag and the capsule.
    property var hostWindow: null
    // What paints behind the surfaces: the app's background or the blurred
    // cover, built by the host, which can reach both.
    property Component backdrop: null

    // One hover sensor for the whole card keeps the capsule open across
    // its buttons.
    HoverHandler { id: cardHover }

    readonly property int surfaceId: Mini.surface
    readonly property bool noShaders: GraphicsInfo.api === GraphicsInfo.Software || GraphicsInfo.api === GraphicsInfo.Null
    // The footer counts the other way: 0 full, 1 compact, 2 micro.
    readonly property int footerMode: surfaceId === 0 ? 2 : surfaceId === 1 ? 1 : 0
    readonly property int footerHeight: surfaceId === 1 ? Card.footerCompact : surfaceId === 0 ? Card.footerMicro : Card.footerFull
    readonly property bool useAppBackground: Theme.ambient && !Mini.backgroundBlur
    readonly property bool backdropOn: useAppBackground || Mini.backgroundBlur
    readonly property bool backdropMounted: Mini.open && backdropOn
    readonly property bool hasTrack: Player.path !== ""

    color: Theme.color.surfaceWindow
    border.width: Card.edge
    border.color: Theme.color.miniEdge
    radius: surfaceId <= 1 ? Card.radiusCondensed : Card.radiusExpanded
    antialiasing: true
    clip: true

    // What a key in the mini player's window does. The surfaces are kept
    // through the same property the capsule writes; playback goes through
    // the same verbs the window's keys use.
    function run(action) {
        switch (action) {
        case "mini.micro": Mini.surface = 0; break
        case "mini.compact": Mini.surface = 1; break
        case "mini.artwork": Mini.surface = 2; break
        case "mini.queue": Mini.surface = 3; break
        case "mini.lyrics": Mini.surface = 4; break
        case "ui.miniPlayer": case "ui.escape": Mini.exit(); break
        case "playback.toggle": Player.toggle(); break
        case "playback.next": Player.next(); break
        case "playback.prev": Player.previous(); break
        case "playback.volumeUp": volume(Metrics.volumeStep); break
        case "playback.volumeDown": volume(-Metrics.volumeStep); break
        case "playback.mute": if (!Player.volumeLocked) Player.chooseMuted(!Player.muted); break
        case "playback.shuffle": Player.toggleShuffle(); break
        case "playback.repeat": Player.cycleRepeat(); break
        case "playback.favorite": Mini.likePlaying(); break
        case "playback.seekForward": Player.nudge(5); break
        case "playback.seekBack": Player.nudge(-5); break
        }
    }
    function volume(by) {
        if (!Player.volumeLocked) {
            Player.chooseVolume(Math.max(0, Math.min(100, Player.volume + by)))
            Player.persistVolume()
        }
    }

    // One rounded mask for either backdrop, shared by the five surfaces.
    // Without shaders it degrades to an unmasked backdrop.
    Item {
        id: backdrop
        objectName: "miniBackdrop"
        anchors.fill: parent
        visible: card.backdropMounted
        clip: true
        layer.enabled: card.backdropMounted && !card.noShaders
        layer.smooth: true
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: backdropMask
            maskThresholdMin: 0.5
            maskSpreadAtMin: 1.0
        }
        Loader {
            anchors.fill: parent
            active: card.backdropMounted && card.backdrop !== null
            sourceComponent: card.backdrop
        }
    }
    Item {
        id: backdropMask
        anchors.fill: parent
        visible: false
        layer.enabled: card.backdropMounted && !card.noShaders
        layer.smooth: true
        // Any opaque fill: the mask reads the alpha.
        Rectangle {
            anchors.fill: parent
            radius: card.radius
            color: Theme.color.surfaceWindow
        }
    }

    // no cursor: the card's free area drags the window, it is not a button
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        onPressed: (mouse) => {
            if ((card.surfaceId === 3 || card.surfaceId === 4) && mouse.y < card.height - card.footerHeight) {
                mouse.accepted = false
                return
            }
            if (card.hostWindow)
                card.hostWindow.startSystemMove()
        }
    }

    // The surface area: absent in micro, where the footer is the card.
    // I release every surface while the retained window is closed, so its
    // covers and data bindings do not follow playback while it is away.
    Item {
        id: surfaceArea
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: card.height - card.footerHeight
        visible: card.surfaceId !== 0
        clip: true

        Loader {
            anchors.fill: parent
            active: Mini.open && card.surfaceId === 1
            sourceComponent: MiniCompactSurface { backdropActive: card.backdropOn }
        }
        Loader {
            anchors.fill: parent
            active: Mini.open && card.surfaceId === 2
            sourceComponent: MiniArtworkSurface { backdropActive: card.backdropOn }
        }
        Loader {
            anchors.fill: parent
            active: Mini.open && card.surfaceId === 3
            sourceComponent: MiniQueueSurface {}
        }
        Loader {
            anchors.fill: parent
            active: Mini.open && card.surfaceId === 4
            sourceComponent: MiniLyricsSurface {}
        }
    }

    // Inset by the hairline on the three edges it touches, so it does not
    // paint over the card's outline, with its corners a hairline tighter
    // so they nest inside the card's. The surface area still ends where
    // the footer starts, so the two overlap by that hairline rather than
    // part.
    MiniFooter {
        id: footer
        objectName: "miniFooter"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: card.border.width
        anchors.rightMargin: card.border.width
        anchors.bottomMargin: card.border.width
        height: card.footerHeight
        backgroundActive: card.backdropOn
        mode: card.footerMode
        cardRadius: card.radius - card.border.width
        hostWindow: card.hostWindow
        windowHovered: cardHover.hovered
    }
}
