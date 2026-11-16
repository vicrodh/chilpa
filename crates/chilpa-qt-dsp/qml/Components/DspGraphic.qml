import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: editor
    property var draft: ({})
    property bool graphic: false
    property var levels: []
    property var frequencies: [31.25, 62.5, 125, 250, 500, 1000, 2000, 4000, 8000, 16000]
    readonly property bool ready: graphic || (draft.filters || []).length === 0
    signal eqChanged(bool enabled)
    signal bandChanged(int index, real gain)
    signal bandPreviewed(int index, real gain)
    signal startRequested()
    signal advancedRequested()
    signal resetRequested()
    signal layoutRequested(var layout)
    spacing: Metrics.gap20
    Text {
        width: parent.width
        text: editor.ready ? I18n.tr("Shape the sound: raise a band to hear more, lower it to hear less.", I18n.rev)
            : I18n.tr("This preset uses precise filters; each band shows its level. To move the bands, create a separate graphic EQ. You can restore your previous EQ.", I18n.rev)
        wrapMode: Text.WordWrap
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
    Row {
        width: parent.width
        spacing: Metrics.gap12
        Toggle { checked: editor.draft.eq_enabled !== false; onToggled: on => editor.eqChanged(on) }
        Text { text: I18n.tr("EQ enabled", I18n.rev); color: Theme.color.textPrimary; font.pointSize: Type.body }
    }
    Flow {
        width: parent.width
        visible: !editor.ready
        spacing: Metrics.gap8
        PillButton { objectName: "dspStartGraphic"; text: I18n.tr("Create graphic EQ", I18n.rev); kind: "primary"; onClicked: editor.startRequested() }
        PillButton { text: I18n.tr("View precise filters", I18n.rev); onClicked: editor.advancedRequested() }
    }
    Flickable {
        id: scroll
        width: parent.width
        height: bands.height + bar.height
        contentWidth: bands.width
        contentHeight: bands.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        Controls.ScrollBar.horizontal: ViewportScrollBar { id: bar }
        Row {
            id: bands
            width: Math.max(scroll.width, editor.frequencies.length * Metrics.dspGraphicBandMinWidth)
            Repeater {
                model: editor.frequencies
                DspGraphicBand {
                    required property int index
                    required property real modelData
                    width: bands.width / Math.max(1, editor.frequencies.length)
                    bandIndex: index
                    frequency: modelData
                    adjustable: editor.ready
                    gain: editor.graphic ? Number(editor.draft.filters?.[index]?.gain_db || 0) : Number(editor.levels[index] || 0)
                    onPreviewed: gain => editor.bandPreviewed(index, gain)
                    onCommitted: gain => editor.bandChanged(index, gain)
                }
            }
        }
    }
    Flow {
        width: parent.width
        spacing: Metrics.gap12
        PillButton { objectName: "dspResetGraphic"; visible: editor.ready; text: I18n.tr("Reset EQ", I18n.rev); onClicked: editor.resetRequested() }
        PillButton { text: I18n.tr("Parametric filters", I18n.rev); onClicked: editor.advancedRequested() }
    }
    DspGraphicLayout { width: parent.width; current: editor.draft.graphic_layout || ({bands: 10, minimum_hz: 31.25, maximum_hz: 16000}); onApplied: layout => editor.layoutRequested(layout) }
}
