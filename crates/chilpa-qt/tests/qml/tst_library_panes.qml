import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/query/qml/Patterns"
import "../../qml/Components"

// The Library's filter columns: year, genre, artist and album by default,
// the columns turned on taking their place in order,
// and the width shared by the ones shown.
TestCase {
    id: test
    name: "LibraryPanes"
    when: windowShown
    visible: true
    width: 1200
    height: 300

    LibraryPanes { id: panes; width: 1200; height: 300 }

    function init() {
        failOnWarning(/Cannot create delegate|Object or context destroyed during incubation|invalid context|TypeError|ReferenceError/)
    }

    function shown() {
        const out = []
        for (let i = 0; i < panes.children.length; ++i) {
            const row = panes.children[i]
            for (let j = 0; j < row.children.length; ++j)
                if (row.children[j].column !== undefined && row.children[j].column !== "")
                    out.push(row.children[j])
        }
        return out.sort((a, b) => a.x - b.x)
    }

    function test_default_columns() {
        panes.shown = ["year", "genre", "artist", "album"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "year,genre,artist,album")
    }

    function test_label_and_composer_follow_the_chosen_order() {
        panes.shown = ["composer", "label", "year", "artist", "album"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "composer,label,year,artist,album")
        const total = shown().reduce((sum, c) => sum + c.width, 0)
        verify(Math.abs(total - (panes.width - 6)) < 2, "the columns share the width: " + total)
    }

    // Rating stands before the artist and draws its entries as stars.
    function test_rating_keeps_its_position_and_draws_its_stars() {
        panes.shown = ["rating", "year", "artist", "album"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "rating,year,artist,album")
        const list = Array.from(shown()[0].children).find(c => c.contentItem !== undefined)
        list.model.append({ key: "4", label: "4 stars", meta: "2", selected: false, excluded: false, all: false })
        tryVerify(() => (findChild(list, "paneStars") || {}).value === 4, 1000, "four stars drawn")
        list.model.clear()
    }

    function test_row_actions_and_excluded_appearance() {
        panes.shown = ["genre"]
        waitForRendering(panes)
        const list = Array.from(shown()[0].children).find(c => c.contentItem !== undefined)
        list.model.append({ key: "", label: "All", meta: "9", selected: true, excluded: false, all: true })
        list.model.append({ key: "Pop", label: "Pop", meta: "3", selected: false, excluded: true, all: false })
        tryVerify(() => list.itemAtIndex(1) !== null)
        const row = list.itemAtIndex(1)
        const minus = findChild(row, "paneExclude")
        verify(minus.visible)
        compare(minus.width, 20)
        compare(minus.height, 20)
        compare(minus.radius, 4)
        verify(findChild(row, "paneLabel").font.strikeout)
        compare(findChild(row, "paneLabel").color, Theme.color.textSecondary)
        verify(!findChild(list.itemAtIndex(0), "paneExclude").visible)
        mouseClick(row, 10, 5)
        compare(JSON.stringify(list.model.choices), "[[1,0]]")
        mouseClick(minus)
        compare(JSON.stringify(list.model.exclusions), "[1]")
        compare(list.model.choices.length, 1, "the minus never sends inclusion")
        mouseClick(row, 10, 5, Qt.LeftButton, Qt.AltModifier)
        compare(JSON.stringify(list.model.choices), "[[1,0],[1,4]]")
        mouseClick(row, 10, 5, Qt.LeftButton, Qt.ControlModifier | Qt.ShiftModifier)
        compare(JSON.stringify(list.model.choices), "[[1,0],[1,4],[1,3]]")
        list.model.setProperty(1, "excluded", false)
        verify(!findChild(row, "paneLabel").font.strikeout)
        list.model.clear()
    }

    function test_every_column_can_be_turned_off() {
        panes.shown = []
        waitForRendering(panes)
        compare(shown().length, 0)
        panes.shown = ["composer"]
        waitForRendering(panes)
        compare(shown().map(c => c.column).join(","), "composer")
    }

    function test_equivalent_columns_keep_their_models_and_rows() {
        panes.shown = ["artist", "album"]
        waitForRendering(panes)
        const original = shown()
        panes.shown = ["artist", "album"]
        waitForRendering(panes)
        compare(shown()[0], original[0])
        compare(shown()[1], original[1])
    }

    function test_reordering_keeps_each_filter_model_and_scroll_position() {
        panes.shown = ["year", "artist", "album"]
        waitForRendering(panes)
        const original = shown(), lists = original.map(column => Array.from(column.children).find(child => child.contentItem !== undefined))
        for (const list of lists) {
            list.model.append(Array.from({length:100},(_,at)=>({key:String(at),label:"Entry " + at,meta:"1",selected:at === 2,excluded:false,all:false})))
            list.contentY = 100
        }
        const positions = lists.map(list => list.contentY)
        panes.shown = ["album", "year", "artist"]
        waitForRendering(panes)
        compare(shown().map(column => column.column), ["album", "year", "artist"])
        compare(shown(), [original[2], original[0], original[1]])
        lists.forEach((list,index) => { compare(list.model.count,100); compare(list.model.get(2).selected,true); fuzzyCompare(list.contentY,positions[index],0.1); list.model.clear() })
    }

    function test_retiring_columns_cancels_cached_rows_before_their_contexts() {
        const entries = Array.from({length:500}, (_, index) => ({
            key:String(index),label:"Fixture "+index,meta:"1",selected:false,excluded:false,all:false
        }))
        for (let cycle = 0; cycle < 12; ++cycle) {
            panes.shown = ["artist", "album", "bits", "rate"]
            waitForRendering(panes)
            for (const column of shown()) {
                const list = Array.from(column.children).find(child => child.contentItem !== undefined)
                list.model.append(entries)
            }
            wait(1)
            panes.shown = ["artist"]
            waitForRendering(panes)
            compare(shown().length, 1)
        }
    }
}
