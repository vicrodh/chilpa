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
    RefineArea { width: gallery.width; height: Metrics.panesHeight; refineMode: true }
    ValuePicker { width: gallery.width; field: "genre" }
    ValuePicker { width: gallery.width; field: "year" }
    ValuePicker { width: gallery.width; field: "rating"; narrow: true }
    PickRow { width: gallery.width; text: "Jazz"; count: "1,940"; mark: "include" }
    Pill { text: "1960s"; on: true }
    SuggestionList { width: gallery.width; typed: "monk" }
    PickValues { width: gallery.width; field: "label" }
    PickYears { width: gallery.width; range: ({ from: 1960, to: 1969 }) }
    PickRatings { width: gallery.width }
    PickPresets { width: gallery.width; lead: "in the last"; choices: [{ text: "30 days", on: true }, { text: "1 year", on: false }] }
    PickWords { width: gallery.width; field: "title" }
    EngineNotes { width: gallery.width; notes: [{ from: "A. Smith", field: "composer", to: "A Smith", kind: "name" }, { from: "rock", field: "genre", to: "Rock & Roll", kind: "near" }] }
    ConsoleLine { width: gallery.width; query: "genre:jazz year:1960..1969" }
    ComplexQuery { width: gallery.width; text: ":filter (genre:jazz | (composer:monk (label:riverside)))" }
    ZeroResults { width: gallery.width }
    DockedNote { width: gallery.width }
    RefineWindow { visible: false; summary: "256 songs · 18 h" }
}
