import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: foot
    property var lyrics: null
    readonly property bool found: lyrics !== null && !!lyrics.document
    property string notice: ""
    topPadding: source.visible || copyNotice.visible || feedback.visible ? Metrics.gap6 : 0
    bottomPadding: source.visible || copyNotice.visible || feedback.visible ? Metrics.gap8 : 0
    spacing: Metrics.gap4

    // What was just done from the options, said for a moment.
    Text {
        id: copyNotice
        objectName: "lyricsNotice"
        text: foot.notice
        visible: text !== ""
        width: parent.width
        wrapMode: Text.Wrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
    Row {
        id: source
        objectName: "lyricsSource"
        visible: foot.found && foot.lyrics.source !== ""
        spacing: Metrics.gap4

        Text {
            text: I18n.tr("Lyrics source:", I18n.rev)
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
        Text {
            objectName: "lyricsSourceName"
            text: foot.found ? foot.lyrics.source : ""
            font.pointSize: Type.small
            font.weight: Type.semibold
            color: Theme.color.accent
        }
    }
    Text {
        id: feedback
        objectName: "lyricsFeedback"
        visible: text !== ""
        text: foot.lyrics && foot.lyrics.notice ? foot.lyrics.notice : ""
        width: parent.width
        wrapMode: Text.Wrap
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }
}
