import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Patterns"
import "../Primitives"

Column {
    id: gallery
    property string sample: "genre:jazz,soul -label:\"blue note\" year:1955..1965"
    width: Metrics.menuMaxWidth * 2 // derived: room for rows and the picker
    spacing: Metrics.gap12
    QueryChipRow { width: gallery.width; query: gallery.sample; onEdited: (query) => gallery.sample = query }
    RefineSwitch {}
    SummaryChip { row: ({ field: "genre", kind: "list", negated: true, values: ["Smooth Jazz"] }) }
    RefinePanel { width: gallery.width; height: Metrics.panesHeight; summary: "256 songs · 18 h" }
    RefineHead { width: gallery.width; conditions: 2; summary: "256 songs · 18 h"; canUndo: true }
    RefineFoot { width: gallery.width; canGroup: true }
    ConditionRow { width: gallery.width; row: ({ field: "genre", kind: "list", negated: false, values: ["Jazz", "Soul"] }) }
    ConditionGroup { width: gallery.width; group: ({ mode: "any", rows: [{ field: "composer", kind: "list", negated: false, values: ["Thelonious Monk"] }] }) }
    Row {
        spacing: Metrics.gap8
        OperatorSwitch { row: ({ field: "year", kind: "range", from: 1960, to: 1969 }) }
        ValueGroup { tokens: [{ label: "1960s", sub: "1960 – 1969", removable: false }] }
        ValueGroup { negated: true; tokens: [{ label: "Smooth Jazz", value: "Smooth Jazz", sub: "", removable: true }] }
        FieldChip { glyph: Icons.tag; text: "Genre" }
        FieldChip { glyph: Icons.calendar; text: "Year"; active: true }
    }
    Row {
        spacing: Metrics.gap8
        SquareButton { tip: "Undo"; glyph: Icons.arrowBackUp }
        SquareButton { tip: "Console"; glyph: Icons.terminal2; checked: true }
        SquareButton { tip: "Remove"; glyph: Icons.trash; small: true }
        RemoveDot { tip: "Remove" }
        Segments { options: [{ text: "all" }, { text: "any" }]; current: 0 }
    }
    FocusPanel { width: gallery.width; height: Metrics.panesHeight }
    RefineArea { width: gallery.width; height: Metrics.panesHeight; refineMode: true }
    FocusCondition {
        row: ({ field: "genre", label: "Genre", include: ["Jazz"], exclude: ["Pop"], range: null, within: null, minimum: null, missing: false })
    }
    FocusValue { text: "Jazz"; excluded: true }
    FocusValues { field: "genre"; width: gallery.width; height: Metrics.panesHeight }
    FocusYears { width: gallery.width; range: [1960, 1979] }
}
