import QtQuick
import "../Theme"

// The widths of a track list's columns, computed once and shared by the
// header and every row, so they cannot disagree (design/03 TrackRow). A
// hidden column measures zero, padding included.
QtObject {
    id: cols

    property real width: 0
    property bool rich: false
    property bool showArtist: true
    property bool showAlbum: true
    property bool showArt: true
    property bool showPlays: false
    property bool showRating: true

    readonly property int pad: rich ? Metrics.rowPadRich : Metrics.rowPadCompact
    readonly property int index: rich ? Metrics.colIndexRich : Metrics.colIndexCompact
    readonly property int art: rich && showArt ? Metrics.colArt : 0
    readonly property int quality: rich ? Metrics.colQualityRich : Metrics.colQualityCompact
    readonly property int rating: showRating ? Metrics.colRating : 0
    readonly property int plays: showPlays ? (rich ? Metrics.colPlaysRich : Metrics.colPlaysCompact) : 0
    readonly property int time: rich ? Metrics.colTimeRich : Metrics.colTimeCompact

    // The flexible columns share what is left, in the design's proportions.
    readonly property real titleShare: 2.4
    readonly property real artistShare: showArtist && !rich ? 1.4 : 0
    readonly property real albumShare: showAlbum ? (rich ? 1.4 : 1.5) : 0
    readonly property real free: Math.max(0, width - pad * 2 - index - art - quality - rating - plays - time) // derived: padding on both sides
    readonly property real unit: free / (titleShare + artistShare + albumShare)

    readonly property real indexX: pad
    readonly property real artX: indexX + index
    readonly property real titleX: artX + art
    readonly property real titleW: unit * titleShare
    readonly property real artistX: titleX + titleW
    readonly property real artistW: unit * artistShare
    readonly property real albumX: artistX + artistW
    readonly property real albumW: unit * albumShare
    readonly property real qualityX: albumX + albumW
    readonly property real ratingX: qualityX + quality
    readonly property real playsX: ratingX + rating
    readonly property real timeX: playsX + plays
}
