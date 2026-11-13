import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Item {
    id: section
    property string key: ""
    property bool recording: false
    readonly property bool narrow: width < Metrics.discoveryTrackNarrow
    readonly property alias scrollView: list.rows
    readonly property bool ready: parts.ready
    TrackListModel { id: parts; kind: "discovery"; key: section.key; filterRev: Library.contentRev + Library.remoteRev; playingPath: Player.path }
    Row {
        id: actions
        spacing: Metrics.gap8
        PillButton { text: I18n.tr("Play", I18n.rev); kind: "primary"; enabled: parts.count > 0; onClicked: parts.playAll(false) }
        PillButton { text: I18n.tr("Add to queue", I18n.rev); enabled: parts.count > 0; onClicked: parts.queueAll() }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.trCount("{} track", "{} tracks", parts.count, I18n.rev); font.pointSize: Type.meta; color: Theme.color.textSecondary }
    }
    TrackList {
        id: list
        objectName: "discoveryTrackList"
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: actions.bottom; anchors.topMargin: Metrics.gap12; anchors.bottom: parent.bottom
        model: parts
        showArtist: !section.recording
        showAlbum: !section.recording && !section.narrow
        showArt: !section.recording
        showRating: !section.recording && !section.narrow
        showSource: !section.narrow
    }
    Text { anchors.centerIn: parent; visible: parts.ready && parts.count === 0; text: I18n.tr("No tracks in this selection", I18n.rev); font.pointSize: Type.body; color: Theme.color.textSecondary }
}
