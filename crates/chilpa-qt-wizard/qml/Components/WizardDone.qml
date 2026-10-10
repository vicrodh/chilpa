import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Step 5: done, and the paths of the files the review offered.
Column {
    id: step
    property var doc: ({})
    readonly property var paths: doc.createdPaths || []
    spacing: Metrics.gap14

    Row {
        spacing: Metrics.gap10
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            glyph: Lucide.circleCheckBig
            iconFamily: Lucide.font
            size: Metrics.iconNav
            color: Theme.color.accent
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("All set", I18n.rev)
            color: Theme.color.textPrimary
            font.pointSize: Type.section
            font.weight: Type.semibold
        }
    }
    Text {
        width: parent.width
        text: I18n.tr("Your DAC is ready for bit-perfect playback. If you applied the config files, restart the audio services (or log out and back in) for them to take effect.", I18n.rev)
        color: Theme.color.textSecondary
        font.pointSize: Type.body
        wrapMode: Text.WordWrap
    }
    Column {
        visible: step.paths.length > 0
        width: parent.width
        spacing: Metrics.gap6
        Text {
            text: I18n.tr("Config files you can create:", I18n.rev)
            color: Theme.color.textSecondary
            font.pointSize: Type.small
            font.weight: Type.medium
        }
        Repeater {
            model: step.paths
            delegate: Text {
                required property string modelData
                width: step.width
                text: modelData
                color: Theme.color.textTertiary
                font.pointSize: Type.small
                font.family: "monospace"
                elide: Text.ElideRight
            }
        }
    }
}
