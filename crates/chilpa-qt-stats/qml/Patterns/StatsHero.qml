import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Components"

// The period at a glance, on the accent's wash: its plays and its time
// against the period before, and beside them its chart, drawn one of two
// ways: the period's bars over the ones before, or the year's mosaic, a
// square a day. Over the chart, what it shows, the arrows that walk it --
// a period at a time for the bars, a year for the mosaic -- and the choice
// between the two.
Rectangle {
    id: hero

    property var report: ({})
    // "bars" or "mosaic".
    property string chart: "bars"
    // The mosaic's document; only read while it is the chart shown.
    property var mosaic: ({})

    signal chartChosen(string chart)
    signal earlierAsked()
    signal laterAsked()
    signal dayChosen(string date)

    readonly property var charts: ["bars", "mosaic"]
    readonly property bool tiled: chart === "mosaic"

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
    Item {
        id: tools
        anchors.left: lead.right
        anchors.leftMargin: Metrics.gap24
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap16
        anchors.top: parent.top
        anchors.topMargin: Metrics.gap16
        height: Charts.chartTools

        Text {
            objectName: "chartCaption"
            anchors.left: parent.left
            anchors.right: walk.left
            anchors.rightMargin: Metrics.gap12
            anchors.verticalCenter: parent.verticalCenter
            elide: Text.ElideRight
            text: hero.tiled
                ? (hero.mosaic.year ? I18n.trArgs("{} · {}", [String(hero.mosaic.year), I18n.trCount("{} play", "{} plays", hero.mosaic.plays || 0, I18n.rev)], I18n.rev) : "")
                : (hero.report.range || "")
            font.pointSize: Type.small
            font.weight: Type.medium
            color: Theme.color.textSecondary
        }
        Row {
            id: walk
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap4

            IconButton {
                objectName: "chartEarlier"
                anchors.verticalCenter: parent.verticalCenter
                enabled: hero.tiled ? hero.mosaic.earlier === true : (hero.report.earlier || "") !== ""
                glyph: Icons.chevronLeft
                tip: hero.tiled ? I18n.tr("Earlier year", I18n.rev) : I18n.tr("Earlier", I18n.rev)
                onClicked: hero.earlierAsked()
            }
            IconButton {
                objectName: "chartLater"
                anchors.verticalCenter: parent.verticalCenter
                enabled: hero.tiled ? hero.mosaic.later === true : (hero.report.later || "") !== ""
                glyph: Icons.chevronRight
                tip: hero.tiled ? I18n.tr("Later year", I18n.rev) : I18n.tr("Later", I18n.rev)
                onClicked: hero.laterAsked()
            }
            Item {
                width: Metrics.gap4
                height: 1
            }
            SegmentedControl {
                objectName: "chartKind"
                anchors.verticalCenter: parent.verticalCenter
                options: [
                    { glyph: Icons.chartBar, tip: I18n.tr("Bars", I18n.rev) },
                    { glyph: Icons.layoutGrid, tip: I18n.tr("Mosaic", I18n.rev) }
                ]
                current: Math.max(0, hero.charts.indexOf(hero.chart))
                onActivated: (index) => hero.chartChosen(hero.charts[index])
            }
        }
    }
    Loader {
        anchors.left: tools.left
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap24
        anchors.top: tools.bottom
        anchors.topMargin: Metrics.gap8
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Metrics.gap16
        sourceComponent: hero.tiled ? tiles : bars
    }
    Component {
        id: bars
        PeriodBars {
            objectName: "heroBars"
            points: hero.report.series || []
        }
    }
    Component {
        id: tiles
        MosaicGrid {
            objectName: "heroMosaic"
            mosaic: hero.mosaic
            chosen: hero.report.anchor || ""
            onDayChosen: (date) => hero.dayChosen(date)
        }
    }
}
