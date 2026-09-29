import QtQuick
import "../Primitives"
import "../Theme"

SelectField {
    id: selector
    required property var detail
    menuWidthLimit: Metrics.albumVersionMenuWidth
    readonly property var versions: JSON.parse(detail.versions || "[]")
    visible: versions.length > 1
    options: versions.map(v => v.label)
    briefs: versions.map(v => v.brief)
    values: versions.map(v => v.id.toString())
    details: versions.map(v => v.detail)
    sources: versions.map(v => v.source)
    value: detail.selectedId.toString()
    onPicked: (value) => detail.selectVersion(value)
}
