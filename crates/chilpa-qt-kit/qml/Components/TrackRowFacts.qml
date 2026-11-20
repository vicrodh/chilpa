import QtQuick
import "../Theme"
import "../Theme/AudioLabels.js" as AudioLabels
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
        Text { text: facts.row.cols.sheet ? facts.row.qualityDetail || facts.row.quality : facts.row.quality; font.pointSize: Type.small; color: Theme.color.textSecondary }
    }
    Text {
        x: facts.row.cols.formatX
        width: Math.max(0, facts.row.cols.format - Metrics.gap12)
        visible: facts.row.cols.format > 0
        anchors.verticalCenter: parent.verticalCenter
        text: facts.row.format || ""
        elide: Text.ElideRight
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    // A pooled row keeps what it had and builds nothing.
    Loader {
        active: facts.row.inViewport ? facts.row.cols.bitsShown || facts.row.cols.rateShown : item !== null
        anchors.fill: parent
        sourceComponent: Item {
            Text {
                objectName: "trackBitDepth"
                x: facts.row.cols.bitsX; width: facts.row.cols.bits - Metrics.gap12
                visible: facts.row.cols.bitsShown
                anchors.verticalCenter: parent.verticalCenter
                text: AudioLabels.display("bits", facts.row.bitDepth) || "—"
                elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textSecondary
            }
            Text {
                objectName: "trackSampleRate"
                x: facts.row.cols.rateX; width: facts.row.cols.rate - Metrics.gap12
                visible: facts.row.cols.rateShown
                anchors.verticalCenter: parent.verticalCenter
                text: AudioLabels.display("rate", facts.row.sampleRate) || "—"
                elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textSecondary
            }
        }
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
        visible: facts.row.cols.time > 0
        text: facts.row.duration
        horizontalAlignment: Text.AlignRight
        font.pointSize: Type.row
        font.features: { "tnum": 1 }
        color: facts.row.rich ? Theme.color.textSecondary : Theme.color.textPrimary
    }
}
