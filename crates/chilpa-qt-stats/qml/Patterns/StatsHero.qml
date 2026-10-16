import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Theme"
import "../Components"

// The period at a glance, on the accent's wash: its plays and its time
// against the period before, and its bars over the ones before.
Rectangle {
    id: hero

    property var report: ({})

    implicitHeight: Charts.heroTall
    radius: Metrics.radiusPanel
    color: Theme.color.statsHero
    border.width: 1
    border.color: Theme.color.strokeHairline

    Column {
        id: lead
        x: Metrics.gap24
        width: Charts.heroLead
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap4

        Text {
            text: I18n.tr("Plays", I18n.rev)
            font.pointSize: Type.label
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            font.capitalization: Font.AllUppercase
            color: Theme.color.textSecondary
        }
        Row {
            spacing: Metrics.gap10
            Text {
                objectName: "heroPlays"
                text: Number(hero.report.plays || 0).toLocaleString(Qt.locale(), "f", 0)
                font.pointSize: Charts.heroNumber
                font.weight: Type.bold
                font.features: { "tnum": 1 }
                color: Theme.color.textPrimary
            }
            ChangeBadge {
                anchors.verticalCenter: parent.verticalCenter
                change: hero.report.plays_change === undefined ? null : hero.report.plays_change
            }
        }
        Item {
            width: 1
            height: Metrics.gap12
        }
        Text {
            text: I18n.tr("Listening time", I18n.rev)
            font.pointSize: Type.label
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            font.capitalization: Font.AllUppercase
            color: Theme.color.textSecondary
        }
        Row {
            spacing: Metrics.gap10
            Text {
                text: hero.report.listened || ""
                font.pointSize: Type.title
                font.weight: Type.semibold
                font.features: { "tnum": 1 }
                color: Theme.color.textPrimary
            }
            ChangeBadge {
                anchors.verticalCenter: parent.verticalCenter
                change: hero.report.listened_change === undefined ? null : hero.report.listened_change
            }
        }
    }
    PeriodBars {
        anchors.left: lead.right
        anchors.leftMargin: Metrics.gap24
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap24
        anchors.top: parent.top
        anchors.topMargin: Metrics.gap24
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap16
        points: hero.report.series || []
    }
}
