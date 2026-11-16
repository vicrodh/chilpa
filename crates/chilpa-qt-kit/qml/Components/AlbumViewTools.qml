import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// View and optional-column choices; the owner persists both selections.
Row {
    id: tools
    property string view: "grid"
    property var columns: []
    signal columnsChosen(var columns)
    signal viewChosen(string view)
    TechnicalColumns { id: technicalOptions }
    spacing: Metrics.gap8
    AlbumColumnsGear {
        visible: tools.view !== "grid"
        anchors.verticalCenter: parent.verticalCenter
        shown: tools.columns
        options: tools.view === "nerdy" ? technicalOptions.choices : []
        onChosen: (columns) => tools.columnsChosen(columns)
    }
    SegmentedControl {
        objectName: "libraryViewSwitcher"
        anchors.verticalCenter: parent.verticalCenter
        options: [
            { glyph: Icons.list, tip: I18n.tr("List", I18n.rev) },
            { glyph: Icons.layoutGrid, tip: I18n.tr("Grid", I18n.rev) },
            { glyph: Lucide.glasses, iconFamily: Lucide.font, tip: I18n.tr("Nerdy", I18n.rev) }
        ]
        current: tools.view === "nerdy" ? 2 : tools.view === "grid" ? 1 : 0
        onActivated: (index) => tools.viewChosen(index === 2 ? "nerdy" : index === 1 ? "grid" : "tracks")
    }
}
