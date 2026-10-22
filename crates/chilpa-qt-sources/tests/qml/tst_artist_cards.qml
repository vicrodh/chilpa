import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Patterns"

TestCase {
    name: "ArtistCards"
    when: windowShown
    visible: true
    width: 1000
    height: 700

    AlbumGridModel { id: everyone; kind: "artists"; key: "album" }
    ArtistCardGrid { id: grid; width: parent.width; height: parent.height; artists: everyone }

    function cards() {
        let found = []
        const walk = item => { if (item.objectName === "sceneArtistCard") found.push(item); for (const child of item.children || []) walk(child) }
        walk(grid.contentItem)
        return found.sort((a, b) => a.artist.title.localeCompare(b.artist.title))
    }

    function test_each_artist_is_a_scene_card_with_a_button_for_each_source() {
        tryVerify(() => cards().length === 3)
        const first = cards()[0]
        compare(first.artist.title, "Artist 0")
        verify(findChild(first, "sceneArtistSource_library"))
        verify(findChild(first, "sceneArtistSource_fixture"))
        compare(first.width, Metrics.sceneCardWidth, "the scene's card, at its size")
    }

    function test_a_source_button_opens_the_artist_there_and_the_card_opens_the_library() {
        tryVerify(() => cards().length === 3)
        const first = cards()[0]
        findChild(first, "sceneArtistSource_fixture").clicked()
        compare(Navigation.went, ["source", "fixture matching-artist-v3:Artist 0"])
        first.activated()
        compare(Navigation.went, ["artist", "Artist 0"])
    }
}
