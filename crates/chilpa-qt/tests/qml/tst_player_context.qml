import QtQuick
import QtTest
import app.chilpa
import app.chilpa.immersive
import "../../qml/Patterns"

TestCase {
    name: "PlayerContext"
    when: windowShown
    visible: true
    width: 900
    height: 500
    PlayerLcd { id: lcd; width: 700 }
    function test_the_cover_grows_on_hover_and_opens_now_playing_on_a_click() {
        lcd.width = 700
        lcd.y = 460
        waitForRendering(lcd)
        const cover = findChild(lcd, "playerCover")
        const preview = findChild(lcd, "playerCoverPreview")
        verify(cover !== null && preview !== null)
        verify(!preview.visible, "nothing over the bar at rest")
        mouseMove(cover, cover.width / 2, cover.height / 2)
        tryVerify(() => preview.visible, 1000, "hovered, the cover grows")
        compare(preview.width, 262, "250 and its frame")
        mouseMove(lcd, lcd.width - 4, lcd.height / 2)
        tryVerify(() => !preview.visible, 1000, "and leaves with the pointer")
        Immersive.open = false
        mouseClick(cover, cover.width / 2, cover.height / 2)
        compare(Immersive.open, true, "a click opens the immersive view")
        compare(Immersive.viewMode, 1)
        compare(Immersive.splitPanel, 4, "on Now Playing")
        Immersive.open = false
        lcd.y = 0
    }
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
