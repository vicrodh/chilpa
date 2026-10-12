import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The breathing discovery mark, progress and slow-connection states.
// Four updates per second only while mounted keep the loading surface calm.
Item {
    id: loading
    property var progress: ({})
    property bool includeSources: false
    signal retry()
    property int elapsed: 0
    readonly property bool longWait: elapsed > 10000 && !timedOut
    readonly property bool timedOut: elapsed > 300000
    readonly property real breathe: (1 - Math.cos(elapsed / 2000 * 2 * Math.PI)) / 2
    readonly property int iconIndex: Math.floor(elapsed / 1500) % 3
    onVisibleChanged: if (visible) elapsed = 0
    Timer { interval: 250; repeat: true; running: loading.visible && !loading.timedOut; onTriggered: loading.elapsed += interval }

    Column {
        visible: !loading.timedOut; anchors.centerIn: parent; spacing: Metrics.gap24
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Metrics.sceneLoadingDisc; height: width; radius: width / Metrics.gap2
            color: Theme.color.surfacePanel
            scale: 1 + 0.06 * loading.breathe; opacity: 0.8 + 0.2 * loading.breathe
            Icon {
                anchors.centerIn: parent; size: Metrics.sceneLoadingIcon
                glyph: loading.iconIndex === 0 ? Icons.music : loading.iconIndex === 1 ? SourceGlyphs.musicbrainz : SourceGlyphs.spiderWeb
                iconFamily: loading.iconIndex === 0 ? "" : SourceGlyphs.font
                color: Theme.color.accent
            }
        }
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Metrics.sceneProgressWidth; height: Metrics.sceneProgressHeight
            radius: Metrics.gap2; color: Theme.color.surfaceRaised
            Rectangle {
                width: parent.width * Math.max(0, Math.min(Metrics.sceneProgressFull, loading.progress.percentage || 0)) / Metrics.sceneProgressFull
                height: parent.height; radius: Metrics.gap2; color: Theme.color.accent
            }
        }
        Column {
            anchors.horizontalCenter: parent.horizontalCenter; spacing: Metrics.gap4
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: (loading.progress.percentage || 0) >= 40
                    ? (loading.includeSources ? I18n.tr("Checking your selected sources…", I18n.rev) : I18n.tr("Preparing results…", I18n.rev))
                    : I18n.tr("Searching MusicBrainz for {genre} artists…", I18n.rev).replace("{genre}", loading.progress.detail || I18n.tr("nearby", I18n.rev))
                font.pointSize: Type.body; color: Theme.color.textSecondary
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: Math.round(loading.progress.percentage || 0) + "%"
                font.pointSize: Type.meta; color: Theme.color.textTertiary
            }
        }
        Text {
            visible: loading.longWait; width: Metrics.sceneWaitWidth
            anchors.horizontalCenter: parent.horizontalCenter
            text: loading.includeSources && (loading.progress.percentage || 0) >= 40
                ? I18n.tr("Checking your selected sources can take a few minutes.", I18n.rev)
                : I18n.tr("This can take a few minutes when MusicBrainz is busy.", I18n.rev)
            wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter
            font.pointSize: Type.meta; color: Theme.color.textTertiary
        }
    }
    Column {
        visible: loading.timedOut; anchors.centerIn: parent
        width: Math.min(Metrics.sceneTimeoutWidth, loading.width - Metrics.sceneErrorInset)
        spacing: Metrics.gap16
        Text {
            width: parent.width; wrapMode: Text.WordWrap; horizontalAlignment: Text.AlignHCenter
            text: loading.includeSources && (loading.progress.percentage || 0) >= 40
                ? I18n.tr("Your selected sources are taking longer than usual. Try again in a moment.", I18n.rev)
                : I18n.tr("This is taking longer than usual. MusicBrainz may be busy. Try again in a moment.", I18n.rev)
            font.pointSize: Type.body; color: Theme.color.textSecondary
        }
        PillButton {
            anchors.horizontalCenter: parent.horizontalCenter
            text: I18n.tr("Retry", I18n.rev)
            onClicked: { loading.elapsed = 0; loading.retry() }
        }
    }
}
