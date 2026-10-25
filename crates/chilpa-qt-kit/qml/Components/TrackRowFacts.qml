import QtQuick
import "../Theme"
import "../Primitives"

// I share the ordinary row's format, rating, play count and duration.
Item {
    id: facts
    required property var row
    readonly property alias ratingStars: stars
    anchors.fill: parent
    Row {
        x: facts.row.cols.qualityX
        anchors.verticalCenter: parent.verticalCenter
        visible: facts.row.cols.quality > 0
        spacing: Metrics.gap6
        QualityIcon { tier: facts.row.tier }
        Text { text: facts.row.quality; font.pointSize: Type.small; color: Theme.color.textSecondary }
    }
    RatingStars {
        id: stars
        x: facts.row.cols.ratingX
        anchors.verticalCenter: parent.verticalCenter
        visible: facts.row.cols.rating > 0
        interactive: false
        value: facts.row.rating
        pointerPreview: facts.row.ratingPreview
    }
    Text {
        x: facts.row.cols.playsX
        width: facts.row.cols.plays - Metrics.gap12
        visible: facts.row.cols.plays > 0
        anchors.verticalCenter: parent.verticalCenter
        text: facts.row.plays
        horizontalAlignment: Text.AlignRight
        font.pointSize: Type.small
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }
    Text {
        x: facts.row.cols.timeX
        width: facts.row.cols.time
        anchors.verticalCenter: parent.verticalCenter
        text: facts.row.duration
        horizontalAlignment: Text.AlignRight
        font.pointSize: Type.row
        font.features: { "tnum": 1 }
        color: facts.row.rich ? Theme.color.textSecondary : Theme.color.textPrimary
    }
}
