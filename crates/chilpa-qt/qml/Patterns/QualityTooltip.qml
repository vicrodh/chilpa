import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// What plays and how it goes out (frontend ADR-0012): the tier, whether the
// engine states it is bit-perfect, the source and the output. Every line
// comes from the engine; nothing here is worked out from the format.
Rectangle {
    width: Metrics.qualityTooltipWidth
    height: lines.implicitHeight + Metrics.gap12 * 2 // derived: padding above and below
    radius: Metrics.radiusMenu
    color: Theme.color.surfaceRaised

    Edge {}

    Column {
        id: lines
        anchors.fill: parent
        anchors.margins: Metrics.gap12
        spacing: Metrics.gap6

        Row {
            spacing: Metrics.gap6
            QualityIcon { tier: Player.qualityTier }
            Text {
                text: Player.bitPerfect ? I18n.tr("Bit perfect", I18n.rev) : I18n.tr("Processed on the way out", I18n.rev)
                font.pointSize: Type.body
                font.weight: Type.semibold
                color: Player.bitPerfect ? Theme.color.accent : Theme.color.textPrimary
            }
        }

        Text {
            width: parent.width
            text: I18n.trArgs("Source: {}", [Player.qualityLong], I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Repeater {
            model: [
                I18n.trArgs("Output: {}", [Player.outputFormat], I18n.rev),
                I18n.trArgs("Device: {}", [Player.device], I18n.rev),
                I18n.trArgs("Route: {}", [Player.route], I18n.rev)
            ]
            Text {
                required property string modelData
                width: parent.width
                text: modelData
                elide: Text.ElideRight
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
        }
    }
}
