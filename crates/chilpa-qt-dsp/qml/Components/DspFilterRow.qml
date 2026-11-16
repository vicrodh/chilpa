import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Rectangle {
    id: band
    property int bandIndex: 0
    property var filter: ({})
    property bool current: false
    readonly property real valueWidth: Math.max(0, (width - Metrics.dspBandIndexWidth - Metrics.toggleCompactWidth - Metrics.dspBandShapeWidth - Metrics.dspChannelWidth - Metrics.button28 - Metrics.gap8 * 7) / 3) // derived: eight columns reserve three equally sized numeric fields
    signal command(string action, var data)
    signal selected()
    height: Metrics.dspFilterRowHeight
    color: current ? Theme.color.stateSelected : "transparent"
    function edit(field, value) { selected(); command("filter_edit", {index: bandIndex, field: field, value: value}) }
    Rectangle { width: parent.width; height: Metrics.hairline; color: Theme.color.strokeHairline }
    Rectangle { width: Metrics.gap2; height: parent.height; color: [Theme.color.accent, Theme.color.statusPositive, Theme.color.statusWarning, Theme.color.statusNegative][Number(band.filter.color || 0) % 4] }
    Row {
        width: parent.width
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8
        PillButton { width: Metrics.dspBandIndexWidth; text: String(band.bandIndex + 1); kind: "ghost"; onClicked: band.selected() }
        Toggle { compact: true; anchors.verticalCenter: parent.verticalCenter; checked: band.filter.enabled !== false; enabled: !band.filter.locked; onToggled: on => band.edit("enabled", on) }
        SelectField {
            width: Metrics.dspBandShapeWidth
            enabled: !band.filter.locked
            options: [I18n.tr("Peak", I18n.rev), I18n.tr("Low shelf", I18n.rev), I18n.tr("High shelf", I18n.rev), I18n.tr("High pass", I18n.rev), I18n.tr("Low pass", I18n.rev), I18n.tr("Notch", I18n.rev), I18n.tr("Band pass", I18n.rev)]
            values: ["peak", "low_shelf", "high_shelf", "high_pass", "low_pass", "notch", "band_pass"]
            value: band.filter.shape || "peak"
            onPicked: value => band.edit("shape", value)
        }
        SelectField { width: Metrics.dspChannelWidth; enabled: !band.filter.locked; options: [I18n.tr("Both", I18n.rev), I18n.tr("Left", I18n.rev), I18n.tr("Right", I18n.rev)]; values: ["all", "left", "right"]; value: band.filter.channel || "all"; onPicked: value => band.edit("channel", value) }
        DspNumber { width: band.valueWidth; value: Number(band.filter.frequency || 1000); minimum: 10; maximum: 24000; label: I18n.tr("Frequency", I18n.rev) + " Hz"; enabled: !band.filter.locked; onCommitted: value => band.edit("frequency", value) }
        DspNumber { width: band.valueWidth; value: Number(band.filter.gain_db || 0); minimum: -24; maximum: 18; label: I18n.tr("Gain", I18n.rev) + " dB"; enabled: !band.filter.locked && ["peak", "low_shelf", "high_shelf"].includes(band.filter.shape); onCommitted: value => band.edit("gain_db", value) }
        DspNumber { width: band.valueWidth; value: Number(band.filter.q || 0.707); minimum: 0.1; maximum: 12; label: "Q"; enabled: !band.filter.locked; onCommitted: value => band.edit("q", value) }
        IconButton { glyph: Icons.x; tip: I18n.tr("Remove filter", I18n.rev); enabled: !band.filter.locked; onClicked: { band.selected(); band.command("filter_remove", {index: band.bandIndex}) } }
    }
}
