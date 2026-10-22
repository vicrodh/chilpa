pragma Singleton
import QtQuick

// Durations (design/01-tokens.md 5). With reduced motion every one is zero,
// so nothing has to ask for itself whether to animate.
QtObject {
    readonly property int busyLoop: 1000
    readonly property int menuLeave: 350
    readonly property int listenRefresh: 400
    // Cards that came with a "Load more" fade in while they are this fresh.
    readonly property int arrival: 1500
    // The Network sidebar sliding open or shut.
    readonly property int networkSlide: 160
    property bool reduced: false

    readonly property int fast: reduced ? 0 : 120
    readonly property int base: reduced ? 0 : 150
    readonly property int panel: reduced ? 0 : 200
    readonly property int page: reduced ? 0 : 300
    // A placeholder dissolving into what replaces it (QBZ's QbzSkeleton).
    readonly property int handover: reduced ? 0 : 180
    // How long a page may wait for its reveal before it is shown whole
    // (QBZ's ContentRouter fadeGuard). Not motion: never reduced.
    readonly property int revealGuard: 1500
    readonly property int ambient: reduced ? 0 : 600
    readonly property int toast: 2200
    // How often a view reads again what moves on another thread: a setup
    // being waited for, a sync. Not motion: never reduced.
    readonly property int poll: 300
    // How long typing pauses before the results page opens, as QBZ's.
    readonly property int searchLive: 300
    readonly property int refresh: 1000
    // Yield a frame between independent Home modules.
    readonly property int moduleStart: 16
    readonly property int accountCheck: 5000
    // How long after the last key what was typed is asked for.
    readonly property int searchSettle: 250
    readonly property int treeSettle: 180
    // How long the flyout that rates shows what was chosen before it goes.
    readonly property int ratingSettle: 450
    // Between one word lighting up and the next, on a line of lyrics timed
    // as a whole.
    readonly property int lyricStagger: reduced ? 0 : 40
    // How far the lyrics clock may run ahead of the last position it got:
    // positions arrive four times a second. Not motion: never reduced.
    readonly property int lyricCarry: 300
    // How long the last line of lyrics lasts when nothing says.
    readonly property int lyricLastLine: 6000
    // A line of lyrics fading as the sung one comes near or leaves it, and
    // the lines moving to keep the sung one in the middle.
    readonly property int lyricDim: reduced ? 0 : 220
    readonly property int lyricScroll: reduced ? 0 : 260
    // Between one asking and the next for the system's window controls.
    readonly property int systemControlsRetry: 200
}
