import QtQuick
import QtQuick.Effects
import "../Theme"

// Exclusive options in a field (design/03 SegmentedControl). `options` is
// a list of { glyph, tip } for icons, or of { text } for words.
Rectangle {
    id: root

    property var options: []
    property int current: 0

    signal activated(int index)

    implicitWidth: row.implicitWidth + Metrics.gap2 * 2 // derived: padding on both sides
    implicitHeight: Metrics.segmentHeight + Metrics.gap2 * 2 // derived: padding above and below
    radius: Metrics.radiusCard
    color: Theme.color.fillField

    // Tab reaches it, and Left and Right move the choice.
    activeFocusOnTab: enabled
    Keys.onPressed: (event) => {
        const step = event.key === Qt.Key_Left ? -1 : event.key === Qt.Key_Right ? 1 : 0
        if (!root.enabled || event.isAutoRepeat || step === 0)
            return
        const next = Math.max(0, Math.min(root.options.length - 1, root.current + step))
        if (next !== root.current)
            root.activated(next)
        event.accepted = true
    }
    FocusRing {}

    Row {
        id: row
        anchors.centerIn: parent
        spacing: Metrics.gap2

        Repeater {
            model: root.options

            Item {
                id: segment
                required property int index
                required property var modelData
                readonly property bool active: index === root.current

                width: segment.modelData.text ? label.implicitWidth + Metrics.gap12 * 2 : Metrics.segmentWidth // derived: padding on both sides
                height: Metrics.segmentHeight
                RectangularShadow {
                    anchors.fill: segmentPanel
                    visible: segment.active
                    radius: segmentPanel.radius
                    offset.y: Metrics.shadowSubtleY
                    blur: Metrics.shadowSubtleBlur
                    color: Theme.color.shadowSegment
                }

                Rectangle {
                    id: segmentPanel
                    anchors.fill: parent
                    radius: Metrics.radiusItem
                    color: segment.active ? (Theme.ambient ? Theme.color.ambientRaised : Theme.color.surfaceRaised) : "transparent"

                    Edge { visible: segment.active }
                }

                Text {
                    id: label
                    anchors.centerIn: parent
                    visible: !!segment.modelData.text
                    text: segment.modelData.text || ""
                    font.pointSize: Type.row
                    font.weight: segment.active ? Type.medium : Type.regular
                    color: segment.active ? Theme.color.textPrimary : Theme.color.textSecondary
                }

                Icon {
                    anchors.centerIn: parent
                    visible: !segment.modelData.text
                    glyph: segment.modelData.glyph || ""
                    size: Metrics.iconMd
                    color: segment.active ? Theme.color.textPrimary : Theme.color.textSecondary
                }

                MouseArea {
                    cursorShape: Qt.PointingHandCursor
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: root.activated(segment.index)
                    onContainsMouseChanged: !segment.modelData.tip ? undefined
                        : containsMouse ? TipState.show(segment.modelData.tip, segment) : TipState.hide(segment)
                }
            }
        }
    }
}
