import QtQuick
import QtQuick.Effects
import app.chilpa
import app.chilpa.dsp
import "qrc:/qt/qml/app/chilpa/dsp/qml/Components" as DspUi
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// What plays and how it goes out (frontend ADR-0012): the tier, whether the
// engine states it is bit-perfect, the source and the output. Every line
// comes from the engine; nothing here is worked out from the format.
Item {
    id: tip
    readonly property var castState: JSON.parse(Cast.snapshot || "{}")
    // Who serves what plays: the library, or a server or source by name.
    readonly property string sourceName: JSON.parse(Sources.snapshot || "{}").playing_source || I18n.tr("Local", I18n.rev)

    width: Metrics.qualityTooltipWidth
    height: lines.implicitHeight + Metrics.gap12 * 2 // derived: padding above and below

    RectangularShadow {
        anchors.fill: panel
        radius: panel.radius
        offset.y: Metrics.shadowElevatedY
        blur: Metrics.shadowElevatedBlur
        color: Theme.color.shadowElevated
    }
    RectangularShadow {
        anchors.fill: panel
        radius: panel.radius
        offset.y: Metrics.shadowKeyY
        blur: Metrics.shadowKeyBlur
        color: Theme.color.shadowElevatedKey
    }
    Rectangle {
        id: panel
        anchors.fill: parent
        radius: Metrics.radiusMenu
        color: Theme.color.surfaceRaised
        Edge {}
    }

    Column {
        id: lines
        anchors.fill: parent
        anchors.margins: Metrics.gap12
        spacing: Metrics.gap6

        Item {
            width: parent.width
            height: Math.max(tierTitle.implicitHeight, status.implicitHeight)

            QualityIcon {
                id: tierIcon
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                tier: Player.qualityTier
            }
            Text {
                id: tierTitle
                anchors.left: tierIcon.right
                anchors.leftMargin: Metrics.gap6
                anchors.right: status.left
                anchors.rightMargin: Metrics.gap8
                anchors.verticalCenter: parent.verticalCenter
                text: Player.qualityTier === 4 ? "DSD"
                    : Player.qualityTier === 3 ? I18n.tr("Hi-Res Lossless", I18n.rev)
                    : Player.qualityTier === 2 ? I18n.tr("Lossless", I18n.rev)
                    : Player.qualityTier === 1 ? I18n.tr("Lossy", I18n.rev)
                    : I18n.tr("Unknown quality", I18n.rev)
                font.pointSize: Type.body
                font.weight: Type.semibold
                elide: Text.ElideRight
                color: Theme.color.textPrimary
            }
            Row {
                id: status
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap4

                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Player.bitPerfect ? Icons.circleCheckFilled : Icons.adjustmentsHorizontal
                    filled: Player.bitPerfect
                    size: Metrics.iconSm
                    color: Player.bitPerfect ? Theme.color.accent : Theme.color.textSecondary
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: Cast.active ? I18n.tr("Original bytes", I18n.rev) : Player.bitPerfect ? I18n.tr("Bit perfect", I18n.rev) : I18n.tr("Processed", I18n.rev)
                    font.pointSize: Type.small
                    font.weight: Type.medium
                    color: Player.bitPerfect ? Theme.color.accent : Theme.color.textSecondary
                }
            }
        }

        Repeater {
            model: [
                { label: I18n.tr("Source", I18n.rev), value: tip.sourceName },
                { label: I18n.tr("Format", I18n.rev), value: Player.qualityLong },
                { label: I18n.tr("Output", I18n.rev), value: Cast.active ? I18n.tr("Original bytes", I18n.rev) : Player.outputFormat },
                { label: I18n.tr("Device", I18n.rev), value: Cast.active ? Cast.destination : Player.device },
                { label: I18n.tr("Route", I18n.rev), value: Cast.active ? tip.castState.protocol || "" : Player.route }
            ].concat(Cast.active ? [{ label: I18n.tr("Status", I18n.rev), value: tip.castState.state || "" }] : [])

            Row {
                required property var modelData
                width: parent.width
                spacing: Metrics.gap8

                Text {
                    width: Metrics.qualityTooltipLabel
                    text: modelData.label
                    font.pointSize: Type.small
                    color: Theme.color.textTertiary
                }
                Text {
                    width: parent.width - Metrics.qualityTooltipLabel - parent.spacing
                    text: modelData.value
                    elide: Text.ElideRight
                    font.pointSize: Type.small
                    color: Theme.color.textSecondary
                }
            }
        }
        DspUi.DspQualityNote { width: parent.width; casting: Cast.active }
        Text {
            visible: Cast.active
            width: parent.width
            text: I18n.tr("Receiver processing unknown", I18n.rev)
            wrapMode: Text.WordWrap
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
    }
}
