import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import app.chilpa.playlists

// How a playlist's tracks are shown, the same for every playlist, ours or a
// source's, as QBZ sorts them, plus Most played: its own order, a field, or
// the last to come in first. Each playlist keeps the order chosen for it.
SortButton {
    id: sort
    objectName: "playlistSort"
    property double playlistId: 0
    // The order and whether it runs its own way, as a track list asks for them.
    readonly property string order: baseOf(value)
    readonly property bool ascending: value === order
    options: [
        {value: "custom", text: I18n.tr("Default", I18n.rev), fixed: true},
        {value: "title", text: I18n.tr("Title", I18n.rev)},
        {value: "artist", text: I18n.tr("Artist", I18n.rev)},
        {value: "album", text: I18n.tr("Album", I18n.rev)},
        {value: "duration", text: I18n.tr("Duration", I18n.rev)},
        {value: "added", text: I18n.tr("Date added", I18n.rev), descending: true},
        {value: "plays", text: I18n.tr("Most played", I18n.rev), descending: true}
    ]
    function restore() { value = playlistId !== 0 ? PlaylistManager.trackOrder(playlistId) : "custom" }
    onPlaylistIdChanged: restore()
    Component.onCompleted: restore()
    onPicked: (chosen) => { value = chosen; PlaylistManager.setTrackOrder(playlistId, chosen) }
}
