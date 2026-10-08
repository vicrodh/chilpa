import QtQuick
import QtTest
import app.chilpa
import "../../qml/Patterns"

TestCase {
    name: "PlayerContext"
    when: windowShown
    visible: true
    width: 900
    height: 500
    PlayerLcd { id: lcd; width: 700 }
    LyricsPanel { id: lyrics; width: 260; height: 400; y: 80 }
    function test_origin_aligns_with_both_context_labels() {
        for (const width of [400, 700]) {
            lcd.width = width
            waitForRendering(lcd)
            const origin = findChild(lcd, "playerOrigin")
            const artist = findChild(lcd, "playerArtist")
            const album = findChild(lcd, "playerAlbum")
            verify(origin.visible)
            const center = origin.mapToItem(lcd, 0, origin.height / 2).y
            verify(Math.abs(center - artist.mapToItem(lcd, 0, artist.height / 2).y) < 1)
            verify(Math.abs(center - album.mapToItem(lcd, 0, album.height / 2).y) < 1)
        }
    }
    function test_prelude_only_hides_followed_timed_lyrics() {
        Player.lyricsJson = JSON.stringify({source:"Fixture",document:{timing:"line",lines:[{text:"First line", start_ms:10000}],layers:[]}})
        Player.lyricsSynced = true
        Player.position = 0
        const options = findChild(lyrics, "lyricsOptions")
        const prelude = findChild(lyrics, "lyricsPrelude")
        const lines = findChild(lyrics, "lyricsList")
        options.follow = true
        tryCompare(prelude, "visible", true)
        compare(lines.visible, false)
        options.follow = false
        tryCompare(prelude, "visible", false)
        compare(lines.visible, true)
        options.follow = true
        Player.position = 10
        tryCompare(prelude, "visible", false)
        Player.position = 0
        Player.lyricsSynced = false
        tryCompare(prelude, "visible", false)
        compare(lines.visible, true)
    }
}
