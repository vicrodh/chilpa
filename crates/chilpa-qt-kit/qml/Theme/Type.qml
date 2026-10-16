pragma Singleton
import QtQuick
import app.chilpa.kit

// Typography (design/01-tokens.md 2). Sizes are given in pixels and applied
// as point sizes, because the design uses half pixels (12.5, 11.5, 10.5) and
// `font.pixelSize` only takes whole ones. At the 96 dpi logical resolution
// Qt reports on Linux and Windows, a point is 4/3 of a pixel. On a Mac Qt
// reports 72, a point is one pixel, and 0.75 drew every text at three
// quarters of its size there.
QtObject {
    readonly property real pointsPerPixel: Qt.platform.os === "osx" ? 1 : 0.75
    readonly property string consoleFamily: ThemeModel.fixedFontFamily()

    // A record's, an artist's or a playlist's name at the head of its page.
    readonly property real display: 23 * pointsPerPixel
    readonly property real title: 28 * pointsPerPixel
    // The ceiling for any title inside a page -- a shelf, a section, a
    // dialog -- is the Home's "Recently played": `section`. Only a page's
    // own heading (`title`) and a record's hero go past it.
    readonly property real section: 19 * pointsPerPixel
    readonly property real titleSmall: section
    readonly property real heroTitle: 18 * pointsPerPixel
    readonly property real bestResult: 15 * pointsPerPixel
    readonly property real body: 13 * pointsPerPixel
    readonly property real bodyLarge: 13.5 * pointsPerPixel
    readonly property real row: 12.5 * pointsPerPixel
    readonly property real meta: 12 * pointsPerPixel
    readonly property real small: 11.5 * pointsPerPixel
    readonly property real label: 11 * pointsPerPixel
    readonly property real micro: 10.5 * pointsPerPixel
    readonly property real lyric: 18 * pointsPerPixel
    // The other sizes lyrics can be read at, chosen in the panel's options.
    readonly property real lyricSmall: 15 * pointsPerPixel
    readonly property real lyricLarge: 22 * pointsPerPixel
    readonly property real lyricXLarge: 26 * pointsPerPixel
    // A translation under its line, as a share of the line's own size.
    readonly property real lyricTranslation: 0.72

    // Weights. The design's 650 falls between DemiBold and Bold; the font
    // database picks the nearest face the system has.
    readonly property int regular: 400
    readonly property int medium: 500
    readonly property int semibold: 600
    readonly property int heading: 650
    readonly property int bold: 700

    // Tracking, in pixels, for the sizes that carry it.
    readonly property real displayTracking: -0.46
    readonly property real titleTracking: -0.56
    readonly property real sectionTracking: -0.19
    readonly property real labelTracking: 0.44
}
