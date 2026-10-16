import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Loading, failure and empty outcomes share the scene's full-page surface.
Item {
    id: sceneState
    property var doc: ({})
    property int artistCount: 0
    signal retry()
    signal more()

    SceneLoading {
        anchors.fill: parent; visible: sceneState.doc.state === "loading"
        progress: sceneState.doc.progress || ({})
        includeSources: sceneState.doc.includeSources === true
        onRetry: sceneState.retry()
    }
    Column {
        anchors.centerIn: parent; spacing: Metrics.gap16
        visible: sceneState.doc.state === "error"
        Text {
            width: Math.min(Metrics.sceneErrorWidth, sceneState.width - Metrics.sceneErrorInset)
            text: sceneState.doc.error || ""; wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter; color: Theme.color.textSecondary
        }
        PillButton {
            anchors.horizontalCenter: parent.horizontalCenter
            text: I18n.tr("Retry", I18n.rev); onClicked: sceneState.retry()
        }
    }
    Column {
        visible: sceneState.doc.state === "ready" && sceneState.artistCount === 0
        anchors.centerIn: parent; spacing: Metrics.gap12
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: sceneState.doc.includeSources === true
                ? I18n.tr("No artists from your selected plugins were found in this scene.", I18n.rev)
                : I18n.tr("No artists from this scene were found.", I18n.rev)
            font.pointSize: Type.body; color: Theme.color.textSecondary
        }
        Text {
            visible: !!sceneState.doc.error
            anchors.horizontalCenter: parent.horizontalCenter
            text: sceneState.doc.error || ""; color: Theme.color.statusNegative
        }
        PillButton {
            visible: sceneState.doc.hasMore === true || sceneState.doc.loadingMore === true
            enabled: sceneState.doc.loadingMore !== true
            anchors.horizontalCenter: parent.horizontalCenter
            text: sceneState.doc.loadingMore === true ? I18n.tr("Loading…", I18n.rev) : I18n.tr("Load more", I18n.rev)
            onClicked: sceneState.more()
        }
    }
}
