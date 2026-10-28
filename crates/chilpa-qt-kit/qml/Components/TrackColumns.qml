import QtQuick
import app.chilpa.kit
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
    property bool showSource: false
    property bool showArt: true
    property bool showPlays: false
    property bool showRating: true
    // The list is kept in its own order: the arrows that move a row go
    // before its number, and the album column gives up the room.
    property bool reorder: false

    readonly property int pad: rich ? Metrics.rowPadRich : Metrics.rowPadCompact
    // How many rows the list has: past three digits, the number column
    // grows a digit's width for each one more, so 10308 is not cut to 0308.
    property int rows: 0
    readonly property int index: (rich ? Metrics.colIndexRich : Metrics.colIndexCompact) + Math.max(0, String(rows).length - 3) * Metrics.gap8
    readonly property int move: !reorder ? 0 : rich ? Metrics.colMoveRich : Metrics.colMoveCompact
    readonly property real fromAlbum: Math.min(move, flex.album)
    readonly property int art: rich && showArt ? Metrics.colArt : 0
    readonly property int sourceWidth: showSource ? Metrics.colSource : 0
    readonly property int quality: rich ? Metrics.colQualityRich : Metrics.colQualityCompact
    readonly property int rating: showRating ? Metrics.colRating : 0
    readonly property int plays: showPlays ? (rich ? Metrics.colPlaysRich : Metrics.colPlaysCompact) : 0
    readonly property int time: rich ? Metrics.colTimeRich : Metrics.colTimeCompact
    // The time ends flush right; what follows it keeps its distance.
    readonly property int afterTime: Metrics.gap16
    readonly property int more: Metrics.colMore

    // The longest texts the compact rows hold: "title" as [title, credit]
    // pairs, "artist" and "album", measured here in the rows' own fonts into
    // what each flexible column wants, or nothing when the list does not say.
    property var widest: ({})
    readonly property FontMetrics rowFont: FontMetrics { font.pointSize: Type.row }
    readonly property FontMetrics creditFont: FontMetrics { font.pointSize: Type.meta }
    function widthOf(texts) { return Math.max(0, ...(texts || []).map(text => rowFont.advanceWidth(text))) }
    property var wants: widest.title === undefined ? ({}) : ({
        title: Math.max(0, ...widest.title.map(pair => rowFont.advanceWidth(pair[0]) + (pair[1] ? Metrics.gap6 + creditFont.advanceWidth(pair[1]) : 0) + (Prefs.trackComposers && pair[2] ? Metrics.gap6 + creditFont.advanceWidth(pair[2]) : 0))),
        artist: widthOf(widest.artist), album: widthOf(widest.album)})
    // A column is never narrower than a header's name.
    readonly property int least: Metrics.gap16 * 4 // derived: room for a header's name

    // The flexible columns share what is left, in the design's proportions,
    // and by what they hold: a column that wants less than its share takes
    // what it wants, the others split the rest, and once every one has what
    // it wants the room left goes by share. Nothing is cut while another
    // column has room to give; without wants, the proportions alone.
    readonly property real titleShare: 2.4
    readonly property real artistShare: showArtist && !rich ? 1.4 : 0
    readonly property real albumShare: showAlbum ? (rich ? 1.4 : 1.5) : 0
    readonly property real free: Math.max(0, width - pad * 2 - index - art - sourceWidth - quality - rating - plays - time - afterTime - more) // derived: padding on both sides
    readonly property var flex: share(free, [["title", titleShare], ["artist", artistShare], ["album", albumShare]], wants || ({}))
    function share(room, shares, wanted) {
        const out = ({title: 0, artist: 0, album: 0})
        const want = key => wanted[key] > 0 ? Math.max(least, wanted[key] + Metrics.gap12) : Infinity // derived: the row's own inset
        let open = shares.filter(entry => entry[1] > 0), left = room
        while (open.length > 0) {
            const total = open.reduce((sum, entry) => sum + entry[1], 0)
            const content = open.filter(entry => want(entry[0]) <= left * entry[1] / total)
            if (content.length === 0) {
                open.forEach(entry => out[entry[0]] = left * entry[1] / total)
                return out
            }
            content.forEach(entry => { out[entry[0]] = want(entry[0]); left -= want(entry[0]) })
            open = open.filter(entry => content.indexOf(entry) < 0)
        }
        const all = shares.filter(entry => entry[1] > 0), total = all.reduce((sum, entry) => sum + entry[1], 0)
        if (left > 0 && total > 0) all.forEach(entry => out[entry[0]] += left * entry[1] / total)
        return out
    }

    readonly property real moveX: pad
    readonly property real indexX: moveX + move
    readonly property real artX: indexX + index
    readonly property real titleX: artX + art
    readonly property real titleW: flex.title - (move - fromAlbum)
    readonly property real artistX: titleX + titleW
    readonly property real artistW: flex.artist
    readonly property real albumX: artistX + artistW
    readonly property real albumW: flex.album - fromAlbum
    readonly property real sourceX: albumX + albumW
    // The row closes in one order everywhere: time, stars, format, and
    // then the heart and the menu.
    readonly property real playsX: sourceX + sourceWidth
    readonly property real timeX: playsX + plays
    readonly property real ratingX: timeX + time + afterTime
    readonly property real qualityX: ratingX + rating
    readonly property real moreX: qualityX + quality
}
