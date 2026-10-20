import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

TestCase {
    id: test
    name: "AlbumMotion"
    when: windowShown
    visible: true
    width: 900; height: 700
    Component {
        id: fixture
        AlbumCarousel {
            width: test.width; size: 4
            model: AlbumGridModel {
                property var cards: [{id:1, title:"First"}, {id:2, title:"Second"}]
                albums: cards.length
                publishRowsOnRevision: false
                function albumId(index, revision) { return cards[index]?.id || 0 }
                function title(index, revision) { return cards[index]?.title || "" }
                Component.onCompleted: {
                    append({first:0, span:1, group:"", revision:0})
                    append({first:1, span:1, group:"", revision:0})
                }
            }
        }
    }
    function cleanup() { AlbumActions.availabilityOf = ({}) }
    function cardIn(row) { return findChild(row, "albumAnimatedCover")?.parent || null }
    function test_external_availability_updates_without_republishing_cards() {
        const shelf = createTemporaryObject(fixture, test)
        const view = findChild(shelf, "albumCarouselRail")
        tryVerify(() => view.itemAtIndex(1) !== null)
        const kept = view.itemAtIndex(1), card = cardIn(kept)
        compare(card.available, true)
        AlbumActions.availabilityOf[2] = false
        compare(card.available, true)
        shelf.model.rev++
        compare(card.available, false)
        compare(kept.revision, 0)
        compare(view.itemAtIndex(1), kept)
        compare(cardIn(kept), card)
    }
    function test_shelf_edits_and_card_metadata_keep_delegates() {
        const shelf = createTemporaryObject(fixture, test)
        const view = findChild(shelf, "albumCarouselRail")
        tryVerify(() => view.itemAtIndex(1) !== null)
        const kept = view.itemAtIndex(1)
        const card = cardIn(kept)
        verify(card)
        compare(card.title, "Second")
        const records = shelf.model
        records.cards = [{id:3, title:"New"}].concat(records.cards)
        records.insert(0, {first:0, span:1, group:"", revision:0})
        records.setProperty(1, "first", 1); records.setProperty(2, "first", 2)
        view.forceLayout()
        compare(view.itemAtIndex(2), kept)
        tryCompare(view.add, "running", false)
        compare(cardIn(kept), card)
        compare(card.title, "Second")
        records.cards[2].title = "New metadata"
        records.setProperty(2, "revision", 1)
        compare(cardIn(kept), card)
        compare(card.title, "New metadata")
        compare(view.add.running, false)
        compare(view.move.running, false)
        records.cards = [records.cards[2], records.cards[0], records.cards[1]]
        records.move(2, 0, 1)
        for (let i = 0; i < records.count; i++) records.setProperty(i, "first", i)
        view.forceLayout()
        compare(view.itemAtIndex(0), kept)
        tryCompare(view.move, "running", false)
        records.cards = records.cards.slice(0, 2)
        records.remove(2)
        view.forceLayout()
        wait(Motion.panel + 50)
        compare(view.count, 2)
        compare(view.itemAtIndex(0), kept)
        compare(cardIn(kept), card)
    }
}
