pragma Singleton
import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// The measures of the mini player, as QBZ's Qt shell has them
// (02-mini §1): the window, the card, the footer at its scales, the
// capsule, the volume pill, the surfaces and the queue rows. Text sizes
// are QBZ's pixels, in the points the kit draws with. Where a measure
// differs between the full and the compact footer it is a pair, indexed
// by the footer's tier: 0 full, 1 compact.
QtObject {
    // The window: fixed per surface, so the desktop can move it and never
    // resize it. The card sits a gutter inside, where the shadow and the
    // hairline draw over the desktop.
    readonly property int windowSpan: 380
    readonly property int heightMicro: 57
    readonly property int heightCompact: 178
    readonly property int heightExpanded: 540
    readonly property int gutter: 6
    readonly property int edge: 1
    readonly property int radiusCondensed: 9
    readonly property int radiusExpanded: 10

    // The footer: in micro it is the whole card.
    readonly property int footerMicro: 45
    readonly property int footerCompact: 64
    readonly property int footerFull: 80
    readonly property var button: [30, 26]
    readonly property var gap: [18, 14]
    readonly property var iconShuffle: [16, 13]
    readonly property var iconSkip: [18, 15]
    readonly property var iconPlay: [20, 16]
    readonly property var controls: [40, 34]
    readonly property var iconVolume: [18, 15]
    readonly property var padSide: [14, 10]
    readonly property var padBottom: [10, 6]
    readonly property var seekHeight: [14, 10]
    readonly property var seekTrack: [3, 2]
    readonly property var timeSize: [11 * Type.pointsPerPixel, 10 * Type.pointsPerPixel]

    // Micro: one elided line that drags, sixteen-pixel controls and a
    // hairline seek, square-ended.
    readonly property int microPad: 8
    readonly property int microTop: 1
    readonly property int microHeader: 17
    readonly property int microControls: 16
    readonly property int microButton: 16
    readonly property int microBetween: 9
    readonly property int microIconShuffle: 8
    readonly property int microIconSkip: 9
    readonly property int microIconPlay: 9
    readonly property int microSeek: 4
    readonly property int microSeekTrack: 1
    readonly property real microText: 11 * Type.pointsPerPixel

    // The seek held where it was asked until the player lands on it.
    readonly property int seekWatchdog: 8000
    readonly property real seekConverge: 0.02

    // The volume pill, opening to the right of its button.
    readonly property int pillLong: 172
    readonly property int pillTall: 34
    readonly property int pillApart: 8
    readonly property int volumeInnerPad: 8
    readonly property int pillBetween: 7
    readonly property int volumeMuteButton: 20
    readonly property int volumeMuteIcon: 14
    readonly property int volumeSlider: 96
    readonly property int volumeNumber: 26
    readonly property real pillNumberText: 10 * Type.pointsPerPixel
    // The layer that closes the pill reaches far past the card and is cut
    // by its clip.
    readonly property int dismissReach: 2000

    // The capsule: a disc that unfolds left into ten buttons.
    readonly property int capsuleTall: 26
    readonly property int capsuleRound: 13
    // 5 tabs x 22 + 4 gaps x 2, a separator of 1, 5 actions x 22 + 4 x 2,
    // and the two gaps of 3 around the separator.
    readonly property int capsuleCluster: 243
    readonly property int capsulePad: 2
    readonly property int capsuleApart: 3
    readonly property int capsuleBetween: 2
    readonly property int capsuleButton: 22
    readonly property int capsuleButtonTall: 20
    readonly property int capsuleButtonRound: 6
    readonly property int capsuleIcon: 13
    readonly property int capsuleMoveIcon: 12
    readonly property int capsuleSlide: 220
    readonly property int capsuleFade: 160
    readonly property int capsuleGrace: 250
    readonly property int separatorTall: 12
    readonly property real idleGlyph: 0.70
    readonly property real idleCapsuleGlyph: 0.65
    readonly property real disabled: 0.3

    // The two display surfaces.
    readonly property int compactPad: 12
    readonly property int coverMin: 40
    readonly property int coverRadiusCompact: 8
    readonly property int coverRadiusArtwork: 10
    readonly property int coverMax: 320
    readonly property int artworkPadSide: 20
    readonly property int artworkPadTop: 16
    readonly property int artworkPadBottom: 12
    readonly property int artworkApart: 12
    readonly property int metaApart: 2
    readonly property real titleText: 13 * Type.pointsPerPixel
    readonly property real bodyText: 12 * Type.pointsPerPixel
    readonly property real metaText: 11 * Type.pointsPerPixel
    readonly property real smallText: 10 * Type.pointsPerPixel
    readonly property real emptyText: 13 * Type.pointsPerPixel

    // The queue: flush rows of 44 with a number where a cover would go.
    readonly property int queueRow: 44
    readonly property int queueRowRound: 4
    readonly property int queueTop: 6
    readonly property int queueBottom: 8
    readonly property int queueCache: 200
    readonly property int queueBar: 3
    readonly property int queueBarIn: 4
    readonly property int queueNumberX: 12
    readonly property int queueNumberSpan: 22
    readonly property int queueApart: 9
    readonly property int queueSidePad: 12

    // The lyrics, and the empty state the queue and the lyrics share.
    readonly property int lyricsPad: 18
    readonly property int emptyIcon: 30
    readonly property int emptyApart: 10
    readonly property int emptyPad: 24

    // The card's own blurred cover: the atmosphere, dimmed on a dark theme,
    // under a veil of the window's surface.
    readonly property real blurDim: 0.54
    readonly property real blurBase: 0.72
    readonly property real blurWarp: 0.20
    readonly property real blurVeil: 0.75
}
