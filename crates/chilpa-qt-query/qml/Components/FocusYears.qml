import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Item {
    id: years
    property var range: null
    property var bounds: []
    signal edited(var from, var to)
    implicitHeight: Metrics.fieldHeight + Metrics.paneRow
    // I read only the year facet while this control is shown; no timer or cache.
    FacetModel { id: facets; column: "year"; rev: Library.rev }
    function refresh() {
        let least = Infinity
        let most = -Infinity
        for (let i = 1; i <= facets.total; ++i) {
            const value = Number(facets.data(facets.index(i, 0), 256))
            if (Number.isFinite(value)) { least = Math.min(least, value); most = Math.max(most, value) }
        }
        bounds = least <= most ? [least, most] : []
    }
    Component.onCompleted: refresh()
    onBoundsChanged: if (slider) Qt.callLater(slider.restore)
    onRangeChanged: if (slider) Qt.callLater(slider.restore)
    Connections {
        target: facets
        function onTotalChanged() { years.refresh() }
        function onModelReset() { years.refresh() }
        function onDataChanged() { years.refresh() }
    }
    Text {
        text: years.bounds.length ? Math.round(slider.first.value) + "–" + Math.round(slider.second.value) : I18n.tr("No results", I18n.rev)
        color: Theme.color.textPrimary
        font.pointSize: Type.meta
    }
    Controls.RangeSlider {
        id: slider
        objectName: "focusYearRange"
        anchors.bottom: parent.bottom
        width: parent.width
        height: Metrics.fieldHeight
        enabled: years.bounds.length > 0
        from: years.bounds[0] ?? 0
        to: Math.max(from, years.bounds[1] ?? 0)
        stepSize: 1
        palette.highlight: Theme.color.accent
        palette.button: Theme.color.surfaceRaised
        palette.mid: Theme.color.strokeHairline
        function restore() {
            if (first.pressed || second.pressed) return
            setValues(years.range && years.range[0] !== null ? years.range[0] : from,
                      years.range && years.range[1] !== null ? years.range[1] : to)
        }
        function commit() { years.edited(Math.round(first.value), Math.round(second.value)) }
        first.onMoved: if (!first.pressed) commit()
        second.onMoved: if (!second.pressed) commit()
        first.onPressedChanged: if (!first.pressed) commit()
        second.onPressedChanged: if (!second.pressed) commit()
    }
}
