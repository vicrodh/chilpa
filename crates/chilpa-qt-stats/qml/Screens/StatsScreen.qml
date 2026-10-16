import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"
import "../Patterns"

// Listening stats (borradores/listening-stats-v1-2026-10-09), with
// Last.fm's weekly report as the model: the day, the week, the month or
// the year that holds a chosen date, or all of it, against the one of its
// kind before; its artists, records and tracks, when and what it was, and
// a few facts. The history, listen by listen, is a page of its own; the
// history goes out as CSV and the report as a picture.
Flickable {
    id: page

    readonly property var kinds: ["day", "week", "month", "year", "all"]
    property string kind: kinds.indexOf(SettingsModel.kept("ui.stats.period")) >= 0 ? SettingsModel.kept("ui.stats.period") : "week"
    // The chosen date; empty is today.
    property string anchor
    readonly property var report: JSON.parse(Stats.report(kind, anchor, Qt.locale().firstDayOfWeek, I18n.rev + Library.contentRev + Library.remoteRev + Library.remoteArtRev) || "{}")
    readonly property real wide: Math.min(width - 2 * Metrics.contentX, Charts.widest) // derived: padding on both sides
    // While the report is drawn into a picture: its caption and its ground.
    property bool picturing: false

    function choose(next) {
        kind = next
        SettingsModel.keep("ui.stats.period", kind)
    }
    // The caption and the ground change the board's size, so the picture
    // waits for the frame that has laid them out.
    function picture() {
        picturing = true
        shot.armed = true
    }

    contentWidth: width
    contentHeight: column.height + Metrics.contentTop + Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ViewportScrollBar {}

    Connections {
        target: Stats
        function onSaved(note) {
            head.note = note
            forget.restart()
        }
    }
    Connections {
        id: shot
        property bool armed: false
        target: page.Window.window
        enabled: armed
        function onFrameSwapped() {
            shot.armed = false
            const to = Stats.picturePath()
            board.grabToImage(function(result) {
                page.picturing = false
                if (result.saveToFile(to))
                    Stats.savePicture(to, page.report.range || "")
            })
        }
    }
    Timer {
        id: forget
        interval: Motion.toast
        onTriggered: head.note = ""
    }

    Column {
        id: column
        x: (page.width - page.wide) / 2 // derived: centered past its widest
        y: Metrics.contentTop
        width: page.wide
        spacing: Metrics.gap16

        StatsHead {
            id: head
            objectName: "statsHead"
            width: parent.width
            report: page.report
            kind: page.kind
            onKindChosen: (next) => page.choose(next)
            onDayChosen: (date) => page.anchor = date
            onHistoryAsked: Navigation.go("listeningHistory", 0)
            onTableAsked: Stats.exportHistory()
            onPictureAsked: page.picture()
        }
        // What the picture holds: the report, and while it is taken, a
        // caption over it and the window's ground around it.
        Item {
            id: board
            readonly property int pad: page.picturing ? Metrics.gap24 : 0
            width: parent.width
            height: content.height + 2 * pad // derived: the ground on both sides

            Rectangle {
                anchors.fill: parent
                visible: page.picturing
                color: Theme.color.surfaceWindow
            }
            Column {
                id: content
                x: board.pad
                y: board.pad
                width: board.width - 2 * board.pad // derived: the ground on both sides
                spacing: Metrics.gap16

                Column {
                    visible: page.picturing
                    spacing: Metrics.gap4

                    Text {
                        text: I18n.tr("Listening stats", I18n.rev)
                        font.pointSize: Type.title
                        font.weight: Type.bold
                        font.letterSpacing: Type.titleTracking
                        color: Theme.color.textPrimary
                    }
                    Text {
                        text: [page.report.range, page.kind === "all" ? "" : page.report.comparison].filter(part => part).join(" · ")
                        font.pointSize: Type.body
                        color: Theme.color.textSecondary
                    }
                }
                StatsHero {
                    width: parent.width
                    report: page.report
                }
                StatsTops {
                    width: parent.width
                    report: page.report
                }
                StatsCharts {
                    width: parent.width
                    report: page.report
                }
                StatsFacts {
                    width: parent.width
                    report: page.report
                }
                // The picture's signature: where it was made.
                Row {
                    objectName: "statsSignature"
                    visible: page.picturing
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: Metrics.gap8

                    Image {
                        objectName: "statsSignatureLogo"
                        width: Charts.signatureLogo
                        height: width
                        anchors.verticalCenter: parent.verticalCenter
                        source: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"
                        sourceSize: Qt.size(Charts.signatureDecode, Charts.signatureDecode)
                        smooth: true
                    }
                    Text {
                        objectName: "statsSignatureText"
                        anchors.verticalCenter: parent.verticalCenter
                        text: I18n.tr("Stats generated by Chilpa", I18n.rev) + " - www.chilpa.app"
                        font.pointSize: Type.meta
                        color: Theme.color.textSecondary
                    }
                }
            }
        }
    }
}
