import QtQuick
import app.chilpa.kit
import "../Theme"

// The widths of a track list's columns, computed once and shared by the
// header and every row, so they cannot disagree (design/03 TrackRow). A
// hidden column measures zero, padding included. It draws nothing: its texts
// are what the fixed columns must hold, set as the header and the rows set
// them, so the widths follow the language, the face and the platform.
Item {
    id: cols
    visible: false

    property bool rich: false
    // Tracks opts into a table; other lists retain their existing layout.
    property bool sheet: false
    property real viewportLeft: 0
    property real viewportRight: width
    function inView(x, width) { return x + width > viewportLeft && x < viewportRight }
    property var optionalColumns: []
    function includes(name) { return !sheet || optionalColumns.indexOf(name) >= 0 }
    readonly property bool showFavorite: includes("favorite")
    readonly property bool showIndex: includes("index")
    readonly property int format: sheet && includes("format") ? named(Metrics.colQualityCompact, formatName) : 0
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
    readonly property int index: !showIndex ? Metrics.rowAction : (rich ? Metrics.colIndexRich : Metrics.colIndexCompact) + Math.max(0, String(rows).length - 3) * Metrics.gap8
    readonly property int move: !reorder ? 0 : rich ? Metrics.colMoveRich : Metrics.colMoveCompact
    readonly property real fromAlbum: Math.min(move, flex.album)
    readonly property int art: rich && showArt && includes("art") ? Metrics.colArt : 0
    readonly property int sourceWidth: showSource && includes("source") ? named(Metrics.colSource, sourceName) : 0
    readonly property int quality: !includes("quality") ? 0 : named(rich ? Metrics.colQualityRich : Metrics.colQualityCompact, qualityName)
    readonly property int rating: showRating && Prefs.ratings && includes("rating") ? named(Metrics.colRating, ratingName) : 0
    readonly property int plays: showPlays && includes("plays") ? named(rich ? Metrics.colPlaysRich : Metrics.colPlaysCompact, playsName) : 0
    // An hour-long track's time fits too: "1:02:47", not cut into the next column.
    readonly property int time: !includes("time") ? 0 : Math.max(named(rich ? Metrics.colTimeRich : Metrics.colTimeCompact, timeName), Math.ceil(clock.implicitWidth))

    // The header's names. A fixed column is the design's width, or as wide
    // as its name in the header's bold (a sorted column's) and the header's
    // inset, so no language cuts it.
    readonly property var names: ({ title: I18n.tr("Title", I18n.rev), artist: I18n.tr("Artist", I18n.rev), album: I18n.tr("Album", I18n.rev),
        source: I18n.tr("Source", I18n.rev), quality: I18n.tr("Quality", I18n.rev), rating: I18n.tr("Rating", I18n.rev),
        format: I18n.tr("Format", I18n.rev), favorite: I18n.tr("Favorite", I18n.rev), plays: I18n.tr("Plays", I18n.rev), time: I18n.tr("Time", I18n.rev) })
    function named(design, name) { return Math.max(design, Math.ceil(name.implicitWidth) + (sheet ? Metrics.gap24 : Metrics.gap8)) }
    Text { id: favoriteName; text: cols.names.favorite; font.pointSize: Type.label; font.weight: Type.bold }
    Text { id: formatName; text: cols.names.format; font.pointSize: Type.label; font.weight: Type.bold }
    Text { id: sourceName; text: cols.names.source; font.pointSize: Type.label; font.weight: Type.bold }
    Text { id: qualityName; text: cols.names.quality; font.pointSize: Type.label; font.weight: Type.bold }
    Text { id: ratingName; text: cols.names.rating; font.pointSize: Type.label; font.weight: Type.bold }
    Text { id: playsName; text: cols.names.plays; font.pointSize: Type.label; font.weight: Type.bold }
    Text { id: timeName; text: cols.names.time; font.pointSize: Type.label; font.weight: Type.bold }
    // The longest time a row says, in the rows' figures.
    Text { id: clock; text: "0:00:00"; font.pointSize: Type.row; font.features: { "tnum": 1 } }
    // The time ends flush right; what follows it keeps its distance.
    readonly property int afterTime: time > 0 ? Metrics.gap16 : 0
    readonly property int favorite: !showFavorite ? 0 : sheet ? named(Metrics.rowAction, favoriteName) : Metrics.rowAction
    readonly property int more: favorite + Metrics.rowAction

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
    readonly property real artistShare: showArtist && (!rich || sheet) ? 1.4 : 0
    readonly property real albumShare: showAlbum ? (rich ? 1.4 : 1.5) : 0
    readonly property real free: Math.max(0, width - pad * 2 - index - art - sourceWidth - format - quality - rating - plays - time - afterTime - more) // derived: padding on both sides
    readonly property real minimumWidth: pad * 2 + index + art + sourceWidth + format + quality + rating + plays + time + afterTime + more + least * 6 // derived: two header widths per mandatory column, with padding on both sides
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
    readonly property real formatX: qualityX + quality
    readonly property real moreX: formatX + format
}
