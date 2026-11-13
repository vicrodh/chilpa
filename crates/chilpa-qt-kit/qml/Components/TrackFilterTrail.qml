import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// At most ten column tags and one search tag; two visible lines at most.
Item {
    id: trail
    property var filters: ({})
    property string search: ""
    property var sourceNames: ({})
    signal edited(string field, string label, Item anchor)
    signal removed(string field)
    signal cleared()
    readonly property var entries: {
        const keys = ["title", "artist", "album", "source", "rating", "format", "quality", "bits", "rate", "favorite"]
        const result = keys.filter(key => {
            const value = filters[key]
            return typeof value === "boolean" || Array.isArray(value) && value.length > 0
        }).map(key => ({field:key, label:names.fields[key], value:names.summary(key, filters[key]), count:Array.isArray(filters[key]) ? filters[key].length : 0}))
        if (search) result.unshift({field:"search", label:names.fields.search, value:search})
        return result
    }
    visible: entries.length > 0
    height: visible ? Math.max(Metrics.hitMin, tags.height) + Metrics.gap8 : 0
    TrackFilterNames { id: names; sourceNames: trail.sourceNames }
    Flickable {
        id: tags
        anchors.left: parent.left
        anchors.right: clear.left
        anchors.rightMargin: Metrics.gap8
        height: Math.min(flow.implicitHeight, Metrics.hitMin * 2 + Metrics.gap6) // derived: two tag lines
        contentWidth: width
        contentHeight: flow.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Controls.ScrollBar.vertical: Controls.ScrollBar { policy: Controls.ScrollBar.AsNeeded }
        Flow {
            id: flow
            width: tags.width
            spacing: Metrics.gap6
            Repeater {
                model: trail.entries
                TrackFilterTag {
                    id: tag
                    required property var modelData
                    objectName: "tracksActive-" + modelData.field
                    label: modelData.label
                    value: modelData.value
                    count: modelData.count || 0
                    maximumWidth: Math.max(Metrics.hitMin, Math.min(Metrics.trackFilterTagWidth, tags.width))
                    onEdited: trail.edited(modelData.field, modelData.label, tag)
                    onRemoved: trail.removed(modelData.field)
                    onFocused: tags.contentY = Math.max(0, Math.min(y, tags.contentHeight - tags.height))
                }
            }
        }
    }
    CompactButton {
        id: clear
        objectName: "tracksClearFilters"
        anchors.right: parent.right
        text: I18n.tr("Clear filters", I18n.rev)
        quiet: true
        onClicked: trail.cleared()
    }
}
