import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"

// Bit depth and sample rate: the usual values as presets, and "more than
// CD" as an open range.
PickPresets {
    id: audio
    // "bits" or "rate".
    property string field: "bits"
    // The range the query holds for the field: {from, to} or null.
    property var held: null
    readonly property bool depth: field === "bits"
    readonly property var steps: depth ? [16, 24, 32] : [44100, 48000, 96000, 192000]
    readonly property int above: depth ? 17 : 48001

    choices: steps.map(n => ({
            text: depth ? I18n.trArgs("{} bits", [String(n)], I18n.rev) : RefineText.kilohertz(n),
            on: !!held && held.from === n && held.to === n,
            change: { op: "range", field: field, from: n, to: n }
        })).concat([{
            text: depth ? I18n.tr("More than 16 bits", I18n.rev) : I18n.tr("More than 48 kHz", I18n.rev),
            on: !!held && held.from === above && !held.to,
            change: { op: "range", field: field, from: above, to: null }
        }])
}
