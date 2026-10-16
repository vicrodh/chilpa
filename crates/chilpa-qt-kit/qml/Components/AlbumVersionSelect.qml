import QtQuick
import "../Primitives"
import "../Theme"

Loader {
    id: selector
    required property var detail
    readonly property var versions: JSON.parse(detail.versions || "[]")
    active: versions.length > 1
    visible: active
    sourceComponent: SelectField {
        menuWidthLimit: Metrics.albumVersionMenuWidth
        options: selector.versions.map(v => v.label)
        briefs: selector.versions.map(v => v.brief)
        values: selector.versions.map(v => v.id.toString())
        details: selector.versions.map(v => v.detail)
        sources: selector.versions.map(v => v.source)
        value: selector.detail.selectedId.toString()
        onPicked: (value) => selector.detail.selectVersion(value)
    }
}
