import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One linked profile, using a small embedded avatar and no network request.
Rectangle {
    id: chip
    required property var person
    readonly property string handle: person.name || ""
    readonly property string url: person.url || ""
    signal activated(string url)
    implicitWidth: row.implicitWidth + Metrics.gap8 * 2 // derived: padding on both sides
    implicitHeight: Metrics.button28
    radius: Metrics.radiusControl
    color: hover.hovered ? Theme.color.stateHover : Theme.color.fillField
    border.width: 1
    border.color: Theme.color.strokeHairline
    activeFocusOnTab: enabled
    Accessible.role: Accessible.Link
    Accessible.name: handle
    Accessible.onPressAction: chip.activated(chip.url)
    Keys.onPressed: event => {
        if (!event.isAutoRepeat && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)) {
            chip.activated(chip.url)
            event.accepted = true
        }
    }
    FocusRing {}
    Row {
        id: row
        anchors.centerIn: parent
        spacing: Metrics.gap6
        Image {
            objectName: "creditAvatar"
            anchors.verticalCenter: parent.verticalCenter
            width: Frame.avatar
            height: width
            // Five 40px images at most; their owners are the visible About body.
            sourceSize: Qt.size(Frame.avatarDecode, Frame.avatarDecode)
            source: chip.person.avatar ? "qrc:/qt/qml/app/chilpa/about/assets/contributors/" + chip.person.avatar : ""
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: chip.handle
            font.pointSize: Type.body
            font.weight: Type.medium
            color: hover.hovered ? Theme.color.accent : Theme.color.textPrimary
        }
        Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.externalLink; size: Metrics.iconXs; color: Theme.color.textTertiary }
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { gesturePolicy: TapHandler.ReleaseWithinBounds; onTapped: chip.activated(chip.url) }
}
