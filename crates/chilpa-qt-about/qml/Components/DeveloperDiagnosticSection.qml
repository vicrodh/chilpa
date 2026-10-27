import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: section
    property var entry: ({})
    property bool expanded: true
    spacing: Metrics.gap8
    PillButton {
        text: section.entry.title || ""
        kind: "ghost"
        glyph: section.expanded ? Icons.chevronUp : Icons.chevronDown
        onClicked: section.expanded = !section.expanded
    }
    DeveloperDiagnosticRow {
        width: parent.width
        visible: section.expanded && section.entry.saved === true
        saved: true
        entry: ({label:I18n.tr("Setting", I18n.rev),saved:I18n.tr("Saved", I18n.rev),runtime:I18n.tr("Runtime", I18n.rev)})
    }
    Repeater {
        model: section.expanded ? section.entry.rows || [] : []
        DeveloperDiagnosticRow {
            required property var modelData
            width: section.width
            entry: modelData
            saved: section.entry.saved === true
        }
    }
}
