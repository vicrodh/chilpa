import QtQuick
import app.chilpa
import app.chilpa.kit
import "../Theme"
import "../Primitives"

Flow {
    id: line
    objectName: "albumGenreLine"
    property var document: ({})
    property bool contrasted: false
    readonly property var entries: (document.items || []).slice(0, Metrics.discoveryPreview)
    readonly property bool hasMore: document.hasMore === true || (document.total || (document.items || []).length) > entries.length
    signal moreRequested()
    visible: entries.length > 0 || hasMore || !!document.error
    spacing: Metrics.gap4
    Repeater {
        model: line.entries
        PillButton {
            required property var modelData
            objectName: "albumGenre:" + modelData.name
            maximumWidth: line.width
            text: modelData.name
            kind: line.contrasted ? "backdrop" : "ghost"
            onClicked: Navigation.goKey("genres", modelData.key)
        }
    }
    PillButton {
        objectName: "albumGenresMore"
        visible: line.hasMore || !!line.document.error
        maximumWidth: line.width
        text: line.document.error ? I18n.tr("Could not read genres", I18n.rev) : I18n.tr("More genres", I18n.rev)
        kind: line.contrasted ? "backdrop" : "ghost"
        onClicked: line.moreRequested()
    }
}
