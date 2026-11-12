import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Column {
    id: section
    spacing: Metrics.gap8
    AlbumGridModel { id: albums; kind: "all" }
    AlbumGrid { width: section.width; height: Metrics.queueMin; model: albums; list: true; technical: true }
    TechnicalCells { width: section.width; height: Metrics.rowHeader; header: true }
    TechnicalCells {
        width: section.width; height: Metrics.rowCompact
        title: "Stored record"; artist: "Artist"
        facts: ({tracks: 12, format: "FLAC", codec: "FLAC", rate: 96000, bits: 24, channels: 2,
            bytes: 104857600, year: 2003, duration_ms: 2400000, source: "local", artwork: true})
    }
    TechnicalCells {
        width: section.width; height: Metrics.rowCompact
        title: "Mixed formats"; artist: "Artist"
        facts: ({tracks: 8, rate: null, bits: null, mixed_fields: ["format", "codec", "rate"]})
    }
    TechnicalColumns { tableWidth: section.width }
    AlbumGroupHeading { width: section.width; text: "Group" }
    AlbumListCells {
        height: Metrics.rowRich
        columns: ["year", "tracks", "rate", "bits", "rating"]
        year: "2003"; tracks: 12; facts: ({rate: 96000, bits: 24})
    }
    AlbumViewTools { columns: ["rate", "bits"]; view: "nerdy" }
    DestructiveSteps { width: section.width; step: 2 }
    PillButton { text: "Destructive confirmation"; onClicked: confirmation.open() }
    ConfirmationDialog {
        id: confirmation
        title: "Remove this example?"
        message: "This removes the selected example from the saved collection."
        confirmText: "Remove example"
        destructive: true
        doubleConfirmation: true
    }
    AlbumGridInteraction { grid: null }
    TrackColumns { id: rowCols; width: section.width }
    Item {
        width: section.width; height: Metrics.rowCompact
        TrackRowFacts { row: ({cols: rowCols, tier: 3, quality: "FLAC", rating: 0, ratingPreview: 0,
            plays: 0, duration: "4:04", rich: false}) }
    }
    TrackSheetState {
        id: sheetState
        settings: ({kept: function(key) { return "" }, keep: function(key, value) {}})
        visits: ({recall: function(key, fallback) { return fallback }, keep: function(key, value) {}})
    }
    TrackColumns { id: sheetCols; width: section.width; sheet: true; optionalColumns: sheetState.columns }
    TrackSheetTools { width: section.width; count: "24 tracks"; shownColumns: sheetState.columns; onColumned: columns => sheetState.keepColumns(columns) }
    TrackSheetHeader {
        width: section.width; cols: sheetCols; filters: sheetState.effective
        onRequested: (field, label, anchor) => sheetFilter.show(field, label, sheetState.filters[field], anchor)
    }
    TrackColumnFilter {
        id: sheetFilter
        model: QtObject {
            property string columnResult: ""
            property bool columnBusy: false
            function requestColumn(field, search, cursor) {
                columnResult = JSON.stringify({field, search, cursor, items:[{key:"Example",label:"Example",count:24}], next:"", more:false})
            }
            function closeColumn() { columnResult = "" }
        }
        onApplied: (field, values) => sheetState.apply(field, values)
    }
    Item {
        id: dragPreview
        property int dropRow: -1
        property int pressedRow: -1
        property bool dragging: false
        property real mouseX: 0
        property real mouseY: 0
        property int dragCount: 1
        property string dragTitle: ""
        property string dragDetail: ""
    }
    TrackDragOverlay { pointer: dragPreview; rows: null; cols: rowCols }
}
