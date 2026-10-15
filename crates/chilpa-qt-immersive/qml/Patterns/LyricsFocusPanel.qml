import QtQuick
import QtQuick.Window
import app.chilpa
import app.chilpa.kit
import app.chilpa.graphics
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"
import "../Theme"

Item {
    id: panel
    property LyricsFeed feed: null
    readonly property bool live: visible && (!Window.window || (Window.window.visibility !== Window.Hidden && Window.window.visibility !== Window.Minimized))
    readonly property bool shaders: Graphics.gpuTier && GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Null
    readonly property bool hasLines: feed !== null && feed.hasLines
    readonly property real columnWidth: Math.min(width - Stage.lyricsPadSide * 2, Stage.lyricColumnMax, width * Stage.lyricColumnShare) // derived: breathing room on both sides
    readonly property real lineSize: Math.round(Math.max(Stage.lineMin, Math.min(columnWidth * Stage.lyricTypeShare, Stage.lineMax))) * Type.pointsPerPixel // derived: pixels to points
    ImmersiveLyricColumn {
        id: verses
        objectName: "immersiveLyricColumn"
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        anchors.topMargin: Stage.lyricsPadTop
        anchors.bottomMargin: Stage.lyricsPadBottom
        width: panel.columnWidth
        visible: panel.hasLines
        feed: panel.feed
        live: panel.live
        shaders: panel.shaders
        lineSize: panel.lineSize
    }
    PillButton {
        objectName: "immersiveLyricsFollow"
        anchors.right: verses.right
        anchors.bottom: verses.bottom
        anchors.bottomMargin: Stage.lyricFollowPad
        visible: panel.hasLines && !panel.feed.waiting && verses.synced && !verses.follow
        text: I18n.tr("Follow lyrics", I18n.rev)
        glyph: Icons.playerPlayFilled
        filled: true
        onClicked: verses.resume()
    }
    Text {
        objectName: "immersiveNoLyrics"
        visible: !panel.hasLines && panel.feed !== null
        anchors.centerIn: parent
        width: panel.columnWidth
        text: I18n.tr("No lyrics", I18n.rev)
        color: Theme.color.immersiveTextGhost
        font.pointSize: Stage.noLyricsText
        font.italic: true
        horizontalAlignment: Text.AlignHCenter
    }
}
