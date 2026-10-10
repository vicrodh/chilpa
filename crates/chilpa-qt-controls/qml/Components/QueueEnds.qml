import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// How long a queue lasts and when it ends: "12 songs 1 hr 5 min -> Ends at
// 21:40", the time local, `remaining` milliseconds after `now`.
Row {
    property string summary
    property real remaining: 0
    property real now: Date.now()
    spacing: Metrics.gap6

    Text {
        objectName: "queuesSummary"
        text: parent.summary
        color: Theme.color.textSecondary
        font.pointSize: Type.meta
    }
    Icon {
        anchors.verticalCenter: parent.verticalCenter
        glyph: Lucide.arrowRight
        iconFamily: Lucide.font
        size: Metrics.iconXs
        color: Theme.color.textTertiary
    }
    Text {
        objectName: "queuesEnds"
        text: I18n.trArgs("Ends at {}", [Qt.formatTime(new Date(parent.now + parent.remaining), "HH:mm")], I18n.rev)
        color: Theme.color.textSecondary
        font.pointSize: Type.meta
    }
}
