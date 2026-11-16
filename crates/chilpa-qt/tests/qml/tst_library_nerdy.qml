import QtQuick
import QtTest
import app.chilpa
import "../../qml/Screens"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

TestCase {
    name: "LibraryNerdy"
    when: windowShown
    visible: true
    width: 1200; height: 700
    Component { id: factory; LibraryScreen {} }
    Loader { id: page; anchors.fill: parent; active: false; sourceComponent: factory }
    function cleanup() { page.active = false; SettingsModel.libraryView = "grid"; AlbumActions.technicalOf = {} }
    function test_column_choices_persist_per_view_and_include_technical_values() {
        const oldNerdy = SettingsModel.kept("ui.library.nerdyColumns")
        const oldList = SettingsModel.kept("ui.library.listColumns")
        try {
            SettingsModel.keep("ui.library.nerdyColumns", "")
            SettingsModel.libraryView = "nerdy"
            page.active = true
            tryVerify(() => page.item !== null)
            let gear = findChild(page.item,"listColumnsButton")
            verify(gear.visible)
            mouseClick(gear)
            let chooser = findChild(gear,"albumColumns")
            tryCompare(chooser,"opened",true)
            waitForRendering(chooser.contentItem)
            verify(findChild(chooser.contentItem,"column-rate") !== null)
            verify(findChild(chooser.contentItem,"column-bits") !== null)
            mouseClick(findChild(chooser.contentItem,"column-codec"))
            tryVerify(() => page.item.nerdyColumns.indexOf("codec") < 0)
            compare(SettingsModel.kept("ui.library.listColumns"),oldList)
            chooser.close()
            page.active = false
            wait(0)
            page.active = true
            tryVerify(() => page.item !== null)
            verify(page.item.nerdyColumns.indexOf("codec") < 0)
            SettingsModel.libraryView = "tracks"
            tryCompare(page.item,"nerdy",false)
            gear = findChild(page.item,"listColumnsButton")
            tryCompare(gear,"shown",page.item.listColumns)
            mouseClick(gear)
            chooser = findChild(gear,"albumColumns")
            tryCompare(chooser,"opened",true)
            waitForRendering(chooser.contentItem)
            compare(chooser.options.length,0)
            tryCompare(findChild(chooser.contentItem,"column-rate"),"checked",false)
            mouseClick(findChild(chooser.contentItem,"column-rate"))
            tryVerify(() => page.item.listColumns.indexOf("rate") >= 0)
            verify(page.item.nerdyColumns.indexOf("codec") < 0)
            chooser.close()
        } finally {
            page.active = false
            SettingsModel.keep("ui.library.nerdyColumns",oldNerdy)
            SettingsModel.keep("ui.library.listColumns",oldList)
        }
    }
    function test_horizontal_bar_stays_above_the_floating_player_band() {
        const previousSpace = Metrics.playerSpace
        try {
            SettingsModel.libraryView = "nerdy"
            page.active = true
            tryVerify(() => page.item !== null)
            const grid = findChild(page.item, "libraryAlbums")
            verify(grid !== null)
            const horizontal = findChild(grid, "albumHorizontalScrollBar")
            verify(horizontal !== null)
            for (const playerSpace of [0, 70, 105]) {
                Metrics.playerSpace = playerSpace
                SettingsModel.libraryView = "nerdy"
                tryCompare(horizontal, "visible", true)
                const reserved = playerSpace + Metrics.playerBottom + Metrics.gap16
                tryVerify(() => horizontal.mapToItem(page.item, 0, horizontal.height).y <= page.item.height - reserved + 0.5, 5000,
                    "the horizontal thumb stays above the player band at height " + playerSpace)
                for (const mode of ["grid", "tracks"]) {
                    SettingsModel.libraryView = mode
                    tryCompare(grid, "technical", false)
                    tryVerify(() => Math.abs(grid.mapToItem(page.item, 0, grid.height).y - page.item.height) < 0.5, 5000,
                        "ordinary " + mode + " keeps its original viewport without an added bottom margin")
                }
            }
        } finally {
            Metrics.playerSpace = previousSpace
        }
    }
    function test_nerdy_lists_albums_instead_of_their_tracks() {
        SettingsModel.libraryView = "nerdy"
        AlbumActions.technicalOf = {1: {tracks: 12, codec: "FLAC", rate: 96000, bits: 24, channels: 2}}
        page.active = true
        tryVerify(() => page.item !== null)
        const grid = findChild(page.item, "libraryAlbums")
        verify(grid !== null)
        compare(grid.technical, true)
        compare(grid.model.kind, "library")
        grid.model.albums = 2
        for (let at = 0; at < 2; ++at) grid.model.append({first: at, span: 1, group: "", revision: 0})
        tryVerify(() => grid.scrollView.itemAtIndex(0) !== null)
        compare(grid.scrollView.count, 2, "one row per album, regardless of its track count")
        const row = grid.scrollView.itemAtIndex(0)
        tryVerify(() => findChild(row, "technicalCell_codec") !== null)
        compare(findChild(row, "technicalCell_album").text, "Record 0")
        compare(findChild(row, "technicalCell_tracks").text, "12")
        compare(findChild(row, "technicalCell_codec").text, "FLAC")
        SettingsModel.libraryView = "grid"
        tryCompare(grid, "technical", false)
        compare(grid.model.albums, 2)
        SettingsModel.libraryView = "tracks"
        compare(grid.list, true)
        compare(grid.technical, false)
    }
}
