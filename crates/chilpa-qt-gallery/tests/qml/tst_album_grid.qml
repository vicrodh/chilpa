import QtQuick
import QtTest
import app.chilpa
import "qrc:/qt/qml/app/chilpa/qml/Components"

TestCase {
    name: "AlbumGridVisibility"
    when: windowShown
    visible: true
    width: 800; height: 500
    AlbumGridModel { id: records }
    Item {
        id: shell
        anchors.fill: parent
        AlbumGrid { id: grid; anchors.fill: parent; model: records }
    }
    function locate(item, predicate) {
        if (predicate(item)) return item
        for (const child of item.children || []) {
            const match = locate(child, predicate)
            if (match) return match
        }
        return null
    }
    function init() {
        shell.visible = true
        grid.list = false
        records.clear()
        records.append({first:0, span:1, group:"", revision:0})
    }
    function test_hiding_a_mounted_grid_retains_its_occupied_card_snapshot() {
        let card = null
        tryVerify(function() {
            card = locate(grid, item => item.qualityInfo !== undefined && item.albumId === 1)
            return card !== null
        })
        compare(card.title, "Record 0")
        shell.visible = false
        compare(card.visible, false)
        compare(card.albumId, 1, "I retain the occupied record while its outgoing page is hidden")
        compare(card.title, "Record 0")
        card.parent.span = 0
        compare(card.albumId, 0, "I still release data when its row no longer contains the record")
    }
    function test_hiding_a_list_retains_its_nearby_row_snapshot() {
        grid.list = true
        let row = null
        tryVerify(function() {
            row = locate(grid, item => item.albumId === 1 && item.subtitle === "Artist" && item.qualityInfo === undefined)
            return row !== null
        })
        shell.visible = false
        compare(row.visible, false)
        compare(row.albumId, 1, "I retain nearby row data independently of ancestor visibility")
        compare(row.title, "Record 0")
    }
}
