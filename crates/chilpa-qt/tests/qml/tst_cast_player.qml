import QtQuick
import QtTest
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../../qml/Patterns"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "CastNowPlaying"
    when: windowShown
    visible: true
    width: 800
    height: 400
    PlayerLcd { id: lcd; width: 780; y: 300 }
    QualityTooltip { id: tip; visible: lcd.qualityHovered }

    function init() {
        Sources.show({playing_parent: "", onAir: ""})
        Sources.asked = []
        Cast.active = false
        Cast.destination = "Test receiver"
        Cast.snapshot = "{}"
        Player.position = 9
        Player.duration = 30
        Player.sought = -1
        Player.albumId = 42
        Player.qualityTier = 2
        Navigation.page = ""
        waitForRendering(lcd)
        mouseMove(lcd, 0, 0)
    }
    function texts(item) {
        let result = item.text !== undefined ? [item.text] : []
        for (const child of item.children || []) result = result.concat(texts(child))
        return result
    }
    function test_remote_uses_local_layout_and_keeps_album_navigation() {
        const indicator = findChild(lcd, "playbackIndicator")
        const album = findChild(lcd, "playerAlbum")
        const cover = findChild(lcd, "playerCover")
        const seek = findChild(lcd, "playerSeek")
        const geometry = [cover.x, cover.y, cover.width, cover.height, seek.x, seek.y, seek.width, album.width]
        compare(indicator.glyph, Icons.waveSquare)
        for (const icon of ["dlna", "cast"]) {
            Cast.snapshot = JSON.stringify({ icon: icon, protocol: icon === "dlna" ? "DLNA / UPnP" : "Chromecast", state: "Playing" })
            Cast.active = true
            wait(0)
            compare(indicator.glyph, icon === "dlna" ? SourceGlyphs.dlna : SourceGlyphs.castAudio)
            compare(indicator.color, Theme.color.casting)
            compare(indicator.iconFamily, SourceGlyphs.font)
            compare(album.text, Player.album)
            compare(cover.source, Player.art)
            compare([cover.x, cover.y, cover.width, cover.height, seek.x, seek.y, seek.width, album.width], geometry)
            mouseClick(album, album.width / 2, album.height / 2)
            tryCompare(Navigation, "page", "album")
            compare(Navigation.selectedId, 42)
            mouseMove(indicator, indicator.width / 2, indicator.height / 2)
            tryCompare(tip, "visible", true)
            const shown = texts(tip)
            verify(shown.includes(Cast.destination))
            verify(shown.includes(icon === "dlna" ? "DLNA / UPnP" : "Chromecast"))
            verify(shown.includes(Player.qualityLong))
            verify(shown.includes("Playing"))
        }
        Cast.active = false
        compare(indicator.glyph, Icons.waveSquare)
    }
    function test_both_podcast_context_links_open_the_parent() {
        Sources.show({playing_parent: "fixture-media", onAir: "fixture-media episode"})
        Player.albumId = 0
        for (const name of ["playerArtist", "playerAlbum"]) {
            const link = findChild(lcd, name)
            waitForRendering(link)
            mouseClick(link, link.width / 2, link.height / 2)
            compare(Sources.asked[Sources.asked.length - 1], ["playing_parent", "", ""])
            compare(Navigation.went, ["source", "fixture-media"])
            Sources.asked = []
        }
    }
    function test_seek_works_after_receiver_reports_duration() {
        Cast.active = true
        Player.duration = 0
        const seek = findChild(lcd, "playerSeek")
        mouseClick(seek, seek.width / 2, seek.height / 2)
        compare(Player.sought, -1)
        Player.duration = 120
        waitForRendering(seek)
        mouseClick(seek, seek.width / 2, seek.height / 2)
        verify(Math.abs(Player.sought - 60) < 1)
        compare(seek.position, Player.sought)
        Player.position = 75
        compare(seek.position, 75)
        Player.qualityTier = 0
        verify(findChild(lcd, "playbackIndicator").visible)
    }
    function test_native_track_seek_click_and_drag() {
        Player.albumId = -3388
        Player.duration = 180
        const seek = findChild(lcd, "playerSeek")
        waitForRendering(seek)
        mouseClick(seek, seek.width / 4, seek.height / 2)
        verify(Math.abs(Player.sought - 45) < 1)
        Player.sought = -1
        mousePress(seek, seek.width / 4, seek.height / 2)
        mouseMove(seek, seek.width * 3 / 4, seek.height / 2)
        compare(Player.sought, -1)
        mouseRelease(seek, seek.width * 3 / 4, seek.height / 2)
        verify(Math.abs(Player.sought - 135) < 1)
    }

}
