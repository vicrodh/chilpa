import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import "../Theme"

// The lyrics' translation, on and off, from a corner of the lyric surface:
// the leave button's glass disc with the language glyph, lit while the
// translation shows. It is there only while the lyrics carry one, as the
// lyrics panel's own button.
Rectangle {
    id: button
    property LyricsFeed feed: null
    // Whether its surface shows lyrics now.
    property bool shown: true
    readonly property bool on: feed !== null && feed.translating
    visible: shown && feed !== null && feed.translation !== null
    width: Stage.capsuleIdle
    height: Stage.capsuleIdle
    radius: width / 2 // derived: a circle
    color: on ? Theme.color.immersiveFill : area.containsMouse ? Theme.color.immersiveNav : Theme.color.immersiveGlass
    border.width: Metrics.hairline
    border.color: Theme.color.immersiveGlassEdge
    Accessible.role: Accessible.Button
    Accessible.name: on ? I18n.tr("Hide translation", I18n.rev) : I18n.tr("Show translation", I18n.rev)

    Text {
        anchors.centerIn: parent
        text: Icons.language
        font.family: Icons.outline
        font.pixelSize: Stage.capsuleIcon
        color: Theme.color.immersiveTextStrong
        opacity: button.on || area.containsMouse ? 1 : Stage.idleGlyph
    }
    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: button.feed.translating = !button.feed.translating
        onContainsMouseChanged: containsMouse ? Kit.TipState.show(button.Accessible.name, button) : Kit.TipState.hide(button)
    }
    onVisibleChanged: if (!visible) Kit.TipState.hide(button)
    Component.onDestruction: Kit.TipState.hide(button)
}
