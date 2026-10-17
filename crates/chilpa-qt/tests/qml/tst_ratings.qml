import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"

TestCase {
    id: test
    name: "Ratings"
    when: windowShown
    visible: true
    width: 1000
    height: 400
    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 250; model: tracks; rich: true; reorderable: true }
    RatingStars {
        id: albumStars
        y: 300
        property int albumId: 1
        value: Ratings.stars("album", albumId, Ratings.rev)
        onRated: (stars) => Ratings.rate("album", albumId, stars)
    }
    RatingStars { id: passiveStars; y: 340; interactive: false }
    function init() {
        mouseMove(test, 900, 380)
        passiveStars.value = 0
        passiveStars.pointerPreview = 0
        Ratings.tracks = ({}); Ratings.albums = ({}); Ratings.fail = false; ++Ratings.rev
        albumStars.albumId = 1
        list.rich = true
        tracks.clear()
        tracks.append({ trackId: 42, rowKind: 0, number: 1, title: "Track", artist: "Band",
            album: "Album", duration: "3:00", plays: 0, quality: "FLAC", tier: 2,
            available: true, playing: false, selected: false, headerLabel: "", headerMeta: "", art: "" })
        tracks.lastAction = ""
        wait(30)
    }
    function row() { return findChild(list, "trackListView").itemAtIndex(0) }
    function test_track_rating_never_selects_plays_or_reorders() {
        const item = row()
        verify(item !== null)
        const x = item.cols.ratingX + 3
        const y = item.height / 2
        mouseClick(item, x, y)
        compare(Ratings.stars("track", 42, 0), 1)
        compare(tracks.lastAction, "")
        mouseClick(item, x, y)
        compare(Ratings.stars("track", 42, 0), 0)
        mouseDoubleClickSequence(item, x, y)
        compare(tracks.lastAction, "")
        mousePress(item, x, y)
        mouseMove(item, x, y + 20)
        mouseRelease(item, x, y + 20)
        compare(tracks.lastAction, "")
    }
    function test_compact_rows_and_recycled_identity() {
        list.rich = false
        wait(10)
        const item = row()
        mouseClick(item, item.cols.ratingX + 3, item.height / 2)
        compare(item.rating, 1)
        tracks.setProperty(0, "trackId", 43)
        tryCompare(item, "rating", 0)
        Ratings.rate("track", 43, 5)
        tryCompare(item, "rating", 5)
        compare(Ratings.stars("track", 42, 0), 1)
    }
    function test_external_previews_fill_only_the_requested_star_cells() {
        waitForRendering(passiveStars)
        const empty = grabImage(passiveStars)
        for (let stars = 1; stars <= 5; ++stars) {
            passiveStars.pointerPreview = stars
            compare(passiveStars.value, 0, "an external preview does not persist a rating")
            waitForRendering(passiveStars)
            const preview = grabImage(passiveStars)
            for (let cell = 0; cell < 5; ++cell) {
                let changed = 0
                for (let x = cell * passiveStars.size; x < (cell + 1) * passiveStars.size; ++x)
                    for (let y = 0; y < preview.height; ++y)
                        if (!Qt.colorEqual(empty.pixel(x, y), preview.pixel(x, y))) changed++
                compare(changed > 0, cell < stars, "only the requested prefix receives filled glyphs")
            }
        }
        passiveStars.pointerPreview = 0
        waitForRendering(passiveStars)
        const cleared = grabImage(passiveStars)
        verify(empty.equals(cleared), "clearing the preview restores all five outlines")
    }
    function test_album_binding_survives_clicks_version_changes_and_errors() {
        mouseClick(albumStars, albumStars.width / 2, albumStars.height / 2)
        compare(albumStars.value, 3)
        albumStars.albumId = 2
        compare(albumStars.value, 0)
        Ratings.rate("album", 2, 4)
        compare(albumStars.value, 4)
        Ratings.fail = true
        mouseClick(albumStars, 1, 1)
        compare(albumStars.value, 4)
        albumStars.albumId = 1
        compare(albumStars.value, 3)
    }
}
