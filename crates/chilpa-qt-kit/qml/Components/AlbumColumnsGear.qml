import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// The gear beside a list of records' list and grid switch: it opens the
// switches for the list's columns (`AlbumColumnsModal`); the screen keeps
// what is chosen.
IconButton {
    id: gear
    objectName: "listColumnsButton"

    property var shown: []
    signal chosen(var columns)

    tip: I18n.tr("Columns", I18n.rev)
    glyph: Icons.settings
    round: true
    onClicked: chooser.open()

    AlbumColumnsModal { id: chooser; shown: gear.shown; onChosen: (columns) => gear.chosen(columns) }
}
