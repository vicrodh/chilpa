import QtQuick
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
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
    AlbumGridInteraction { grid: null }
    TrackColumns { id: rowCols; width: section.width }
    Item {
        width: section.width; height: Metrics.rowCompact
        TrackRowFacts { row: ({cols: rowCols, tier: 3, quality: "FLAC", rating: 0, ratingPreview: 0,
            plays: 0, duration: "4:04", rich: false}) }
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
