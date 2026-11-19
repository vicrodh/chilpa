import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// A year a square a day, a column a week, each as strong as the day was
// played against the year's busiest; the months named above and every
// other day of the week beside. The facade lays it out, so this only
// places squares. The square under the pointer says its day and its
// count; a click chooses that day. Where the year does not fit at the
// smallest a square is drawn, it scrolls sideways, and opens on the week
// of the day chosen, or of the last day there is.
Item {
    id: grid

    // {columns, cells: [{date, column, row, plays, level, later, tip}],
    //  months: [{label, column}], rows: [label], levels}
    property var mosaic: ({})
    property color tint: Theme.color.accent
    // The day the report is about, "2026-10-09".
    property string chosen

    signal dayChosen(string date)

    readonly property var cells: mosaic.cells || []
    readonly property int columns: Math.max(1, mosaic.columns || 1)
    readonly property int levels: Math.max(2, mosaic.levels || 2)
    // A square and the gap after it: as large as fits, between its
    // smallest size and its own.
    readonly property int step: Math.max(Charts.mosaicLeast + Charts.mosaicApart, Math.min(Charts.mosaicCell + Charts.mosaicApart, Math.floor((width - Charts.mosaicDays) / columns), Math.floor((height - Charts.mosaicMonths) / 7))) // derived: seven days down
    // The column to open on: the chosen day's, or the last day's there is.
    readonly property int openColumn: {
        let column = 0
        for (let i = 0; i < cells.length; i++) {
            if (cells[i].later)
                break
            column = cells[i].column
            if (cells[i].date === chosen)
                break
        }
        return column
    }
    function settle() {
        view.contentX = Math.max(0, Math.min(view.contentWidth - view.width, (openColumn + 1) * step - view.width))
    }
    onOpenColumnChanged: settle()
    readonly property int cell: step - Charts.mosaicApart
    // The square under the pointer, or -1; shown again under a still pointer, it says itself again.
    property int hovered: -1
    onVisibleChanged: if (visible && pointer.hovered) pointer.place()
    // Where each square is in `cells`, by its place in the grid.
    readonly property var places: {
        const at = {}
        for (let i = 0; i < cells.length; i++)
            at[cells[i].column * 7 + cells[i].row] = i // derived: seven days a column
        return at
    }

    implicitHeight: Charts.mosaicMonths + 7 * (Charts.mosaicCell + Charts.mosaicApart) // derived: seven days down

    Repeater {
        model: grid.mosaic.rows || []

        Text {
            required property var modelData
            required property int index
            y: Charts.mosaicMonths + index * grid.step + (grid.cell - height) / 2 // derived: centered on its row
            visible: index % 2 === 0
            text: modelData
            font.pointSize: Type.micro
            color: Theme.color.textTertiary
        }
    }
    Flickable {
        id: view
        objectName: "mosaicView"
        x: Charts.mosaicDays
        width: grid.width - Charts.mosaicDays
        height: Charts.mosaicMonths + 7 * grid.step // derived: seven days down
        contentWidth: grid.columns * grid.step
        contentHeight: height
        clip: true
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentWidth > width
        onWidthChanged: grid.settle()
        onContentWidthChanged: grid.settle()

        Repeater {
            model: grid.mosaic.months || []

            Text {
                required property var modelData
                x: modelData.column * grid.step
                text: modelData.label
                font.pointSize: Type.micro
                color: Theme.color.textTertiary
            }
        }
        Item {
            id: field
            objectName: "mosaicField"
            y: Charts.mosaicMonths
            width: grid.columns * grid.step
            height: 7 * grid.step // derived: seven days down

            Repeater {
                model: grid.cells

                Rectangle {
                    required property var modelData
                    required property int index
                    x: modelData.column * grid.step
                    y: modelData.row * grid.step
                    width: grid.cell
                    height: grid.cell
                    radius: Math.min(Charts.mosaicRound, width / 2) // derived: never rounder than a circle
                    visible: !modelData.later
                    color: modelData.level > 0 ? grid.tint : Theme.color.statsPrevious
                    opacity: modelData.level > 0 ? Charts.mosaicFaint + (1 - Charts.mosaicFaint) * (modelData.level - 1) / (grid.levels - 1) : 1
                    border.width: modelData.date === grid.chosen || index === grid.hovered ? 1 : 0
                    border.color: Theme.color.textPrimary
                }
            }
            // Where the tooltip points: the square under the pointer.
            Item {
                id: marker
                readonly property var day: grid.hovered >= 0 ? grid.cells[grid.hovered] : null
                x: day ? day.column * grid.step : 0
                y: day ? day.row * grid.step : 0
                width: grid.cell
                height: grid.cell
            }
            HoverHandler {
                id: pointer
                cursorShape: grid.hovered >= 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
                function place() {
                    const column = Math.floor(point.position.x / grid.step)
                    const row = Math.floor(point.position.y / grid.step)
                    const found = hovered && column >= 0 && row >= 0 && row < 7 ? grid.places[column * 7 + row] : undefined // derived: seven days a column
                    const next = found === undefined || grid.cells[found].later ? -1 : found
                    // TipState lets go of the marker when the grid hides: the same square asks again.
                    if (next === grid.hovered && (next < 0 || TipState.target === marker))
                        return
                    TipState.hide(marker)
                    grid.hovered = next
                    if (next >= 0)
                        TipState.show(I18n.trArgs("{} · {}", [grid.cells[next].tip, I18n.trCount("{} play", "{} plays", grid.cells[next].plays, I18n.rev)], I18n.rev), marker)
                }
                onPointChanged: place()
                onHoveredChanged: place()
            }
            TapHandler {
                onTapped: {
                    if (grid.hovered >= 0)
                        grid.dayChosen(grid.cells[grid.hovered].date)
                }
            }
        }
    }
}
