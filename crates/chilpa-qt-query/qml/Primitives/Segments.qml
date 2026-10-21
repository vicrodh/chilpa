import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// A segmented choice on a field-coloured track (2 padding): the chosen
// segment is raised. One size for the panel's all/any and the operator
// (20 tall, radius 5), another for the Columns | Refine switch (26, 6).
// A segment may carry a glyph and a badge count; `bare` hides its words.
Rectangle {
    id: track
    // [{text, glyph?, badge?}]
    property var options: []
    property int current: -1
    property int tall: Refine.segTall
    property int pad: Refine.segPad
    property int round: Refine.segRound
    property real textSize: Type.meta
    property bool bare: false
    signal activated(int index)

    implicitWidth: line.implicitWidth + Refine.segTrackPad * 2 // derived: the track on both sides
    implicitHeight: tall + Refine.segTrackPad * 2 // derived: the track above and below
    radius: round + Refine.segTrackPad
    color: Theme.color.fillField

    Row {
        id: line
        x: Refine.segTrackPad
        y: Refine.segTrackPad
        spacing: Refine.segTrackPad
        Repeater {
            model: track.options
            Rectangle {
                id: segment
                required property var modelData
                required property int index
                readonly property bool on: index === track.current
                objectName: "segment"
                width: content.implicitWidth + track.pad * 2 // derived: padding on both sides
                height: track.tall
                radius: track.round
                color: on ? Theme.color.surfaceRaised : "transparent"
                border.width: on ? Metrics.hairline : 0
                border.color: Theme.color.strokeHairline
                Row {
                    id: content
                    anchors.centerIn: parent
                    spacing: Refine.modeApart
                    Icon {
                        visible: !!segment.modelData.glyph
                        anchors.verticalCenter: parent.verticalCenter
                        glyph: segment.modelData.glyph || ""
                        size: Refine.modeIcon
                        color: segment.on ? Theme.color.textPrimary : Theme.color.textSecondary
                    }
                    Text {
                        visible: !track.bare || !segment.modelData.glyph
                        anchors.verticalCenter: parent.verticalCenter
                        text: segment.modelData.text
                        font.pointSize: track.textSize
                        color: segment.on ? Theme.color.textPrimary : Theme.color.textSecondary
                    }
                    Rectangle {
                        objectName: "segmentBadge"
                        visible: (segment.modelData.badge || 0) > 0
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.max(height, badge.implicitWidth + Refine.badgePad * 2) // derived: padding on both sides
                        height: Refine.badgeTall
                        radius: Refine.badgeRound
                        color: Theme.color.accent
                        Text {
                            id: badge
                            anchors.centerIn: parent
                            text: String(segment.modelData.badge || "")
                            font.pointSize: Refine.badgeText
                            font.weight: Type.bold
                            color: Theme.color.textOnAccent
                        }
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: track.activated(segment.index)
                }
            }
        }
    }
}
