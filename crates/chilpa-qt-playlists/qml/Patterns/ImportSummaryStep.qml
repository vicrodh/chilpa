import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// What the import made, one card per destination, and the songs it could
// not place, folded away until asked for.
Flickable {
    id: step
    property var status: ({})
    property var places: ({})
    property bool missingShown: false
    signal opened(var output, var part)
    clip: true
    contentWidth: width
    contentHeight: column.height
    boundsBehavior: Flickable.StopAtBounds
    Controls.ScrollBar.vertical: ViewportScrollBar { gutter: Metrics.pageGutter }
    Column {
        id: column
        width: step.width
        spacing: Metrics.gap12
        Repeater {
            model: step.status.outputs || []
            ImportResultCard {
                required property var modelData
                required property int index
                objectName: "importResult_" + index
                width: column.width
                output: modelData
                name: (step.places[modelData.target] || {}).name || modelData.target
                sourceInfo: (step.places[modelData.target] || {}).info || ({})
                onOpened: part => step.opened(modelData, part)
            }
        }
        PillButton {
            objectName: "importMissing"
            kind: "ghost"
            visible: (step.status.row_count || 0) > 0
            glyph: step.missingShown ? Lucide.chevronDown : Lucide.chevronRight; iconFamily: Lucide.font
            text: I18n.trArgs("Not found ({})", [String(step.status.row_count || 0)], I18n.rev)
            onClicked: step.missingShown = !step.missingShown
        }
        Column {
            objectName: "importMissingRows"
            width: column.width
            visible: step.missingShown
            Repeater {
                model: step.missingShown ? step.status.rows || [] : []
                ImportReviewRow {
                    required property var modelData
                    required property int index
                    width: column.width; rowIndex: index
                    number: modelData.index; original: modelData.original
                }
            }
        }
    }
}
