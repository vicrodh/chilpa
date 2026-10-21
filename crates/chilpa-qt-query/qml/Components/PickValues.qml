import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// A field's values with what each would leave (F3): "No genre" first while
// the field can be empty, then every value, at most 196 tall and scrolling.
// Only an open list asks for its counts.
Column {
    id: values
    property string field
    property string search: ""
    // The values the query already includes and keeps out, and whether
    // "has none" stands.
    property var included: []
    property var excluded: []
    property bool none: false
    signal edit(var change)

    spacing: 0
    FacetModel { id: facets; column: values.field; rev: Library.rev; query: values.search }
    PickRow {
        objectName: "pickNone"
        visible: RefineText.noneFields.indexOf(values.field) >= 0
        width: parent.width
        text: RefineText.noneLabel(values.field)
        italic: true
        actions: false
        mark: values.none ? "include" : "none"
        onIncluded: values.edit({ op: "none", field: values.field, on: !values.none })
    }
    Rectangle { visible: RefineText.noneFields.indexOf(values.field) >= 0; width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    ListView {
        objectName: "pickValues"
        width: parent.width
        height: Math.min(contentHeight, Refine.pickListMax)
        clip: true
        reuseItems: true
        boundsBehavior: Flickable.StopAtBounds
        model: facets
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        delegate: PickRow {
            required property string key
            required property string label
            required property string albums
            required property bool all
            width: ListView.view.width
            height: all ? 0 : Refine.pickRow
            visible: !all
            text: label
            count: albums
            mark: values.included.indexOf(key) >= 0 ? "include" : values.excluded.indexOf(key) >= 0 ? "exclude" : "none"
            onIncluded: values.edit({ op: "value", field: values.field, value: key, state: mark === "include" ? "none" : "include" })
            onExcluded: values.edit({ op: "value", field: values.field, value: key, state: mark === "exclude" ? "none" : "exclude" })
        }
    }
}
