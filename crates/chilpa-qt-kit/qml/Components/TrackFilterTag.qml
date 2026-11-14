import QtQuick
import app.chilpa.kit
import "../Theme"
import "../Primitives"

// One column per tag keeps the trail bounded even with large selections.
Rectangle {
    id: tag
    property string label: ""
    property string value: ""
    property int count: 0
    property real maximumWidth: Metrics.trackFilterTagWidth
    signal edited()
    signal removed()
    signal focused()
    height: Metrics.hitMin
    implicitWidth: title.implicitWidth + close.width + total.width + Metrics.gap6 + Metrics.gap10 * 2 // derived: text insets
    width: Math.min(implicitWidth, maximumWidth)
    radius: Metrics.radiusRow
    color: hover.hovered ? Theme.color.stateHover : Theme.color.fillField
    Item {
        id: edit
        anchors.left: parent.left
        anchors.right: total.left
        height: parent.height
        activeFocusOnTab: true
        onActiveFocusChanged: if (activeFocus) tag.focused()
        Accessible.role: Accessible.Button
        Accessible.name: tag.label + ": " + tag.value
        Accessible.onPressAction: tag.edited()
        Keys.onReturnPressed: tag.edited()
        Keys.onEnterPressed: tag.edited()
        Keys.onSpacePressed: tag.edited()
        Text {
            id: title
            anchors.fill: parent
            anchors.leftMargin: Metrics.gap10
            anchors.rightMargin: Metrics.gap4
            verticalAlignment: Text.AlignVCenter
            text: tag.label + ": " + tag.value
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
            elide: Text.ElideRight
        }
        FocusRing {}
        HoverHandler {
            id: hover
            cursorShape: Qt.PointingHandCursor
            onHoveredChanged: hovered ? TipState.show(tag.label + ": " + tag.value, tag) : TipState.hide(tag)
        }
        TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: tag.edited() }
    }
    Text {
        id: total
        anchors.right: close.left
        anchors.verticalCenter: parent.verticalCenter
        width: tag.count > 1 ? implicitWidth + Metrics.gap6 : 0
        text: tag.count > 1 ? "(" + tag.count + ")" : ""
        font.pointSize: Type.label
        color: Theme.color.textTertiary
    }
    CompactButton {
        id: close
        objectName: "tracksRemoveFilter"
        anchors.right: parent.right
        width: Metrics.hitMin
        glyph: Icons.x
        tip: I18n.tr("Clear filter", I18n.rev) + ": " + tag.label
        quiet: true
        onActiveFocusChanged: if (activeFocus) tag.focused()
        onClicked: tag.removed()
    }
}
