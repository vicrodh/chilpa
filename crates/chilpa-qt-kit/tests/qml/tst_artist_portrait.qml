import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    name: "ArtistPortrait"
    when: windowShown
    visible: true
    width: 400; height: 400
    Component { id: maker; ArtistPortrait { width:200; height:200 } }
    function test_a_manual_image_takes_precedence_without_a_metadata_job() {
        const portrait = createTemporaryObject(maker, this, {name:"Artist",chosen:"file:///chosen.jpg"})
        const resolver = findChild(portrait, "portraitResolver")
        compare(resolver.active, false)
        compare(portrait.source.toString(), "file:///chosen.jpg")
        resolver.source = "file:///automatic.jpg"
        compare(portrait.source.toString(), "file:///chosen.jpg")
    }
    function test_offscreen_and_recycled_cards_cancel_metadata_and_release_images() {
        const portrait = createTemporaryObject(maker, this, {name:"First artist"})
        const resolver = findChild(portrait, "portraitResolver")
        compare(resolver.active, true)
        resolver.source = "file:///first.jpg"
        compare(portrait.source.toString(), "file:///first.jpg")
        portrait.activeImages = false
        compare(resolver.active, false)
        compare(portrait.source.toString(), "")
        portrait.name = "Second artist"
        compare(resolver.name, "Second artist")
        portrait.activeImages = true
        compare(resolver.active, true)
        compare(portrait.source.toString(), "")
    }
}
