import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.playlists
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// Under a playlist's name field: an emoji or a symbol may be refused by a
// source, which then keeps only part of the name.
Text {
    property string name
    objectName: "playlistNameNote"
    visible: PlaylistManager.nameIsUnusual(name)
    text: I18n.tr("Some sources accept only part of these characters and may cut the name short.", I18n.rev)
    wrapMode: Text.Wrap
    textFormat: Text.PlainText
    font.pointSize: Type.small
    color: Theme.color.textTertiary
}
