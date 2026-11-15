import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

Item {
    id: section
    property string key: ""
    property string filter: ""
    property string order: "name"
    property Component cards
    readonly property var scrollView: grid.item
    readonly property bool ready: people.ready
    readonly property string error: people.error
    function retry() { people.retry() }
    AlbumGridModel { id:people; objectName:"discoveryArtistModel"; kind:"discoveryArtists"; key:section.key; filter:section.filter; order:section.order; columns:grid.item?.columns || 0; contentRev:Library.contentRev + Library.remoteRev }
    Loader { id:grid; anchors.fill:parent; sourceComponent:section.cards; readonly property var artists:people }
    Text { anchors.centerIn:parent; visible:people.ready && people.albums === 0; text:I18n.tr("No artists in this selection",I18n.rev); font.pointSize:Type.body; color:Theme.color.textSecondary }
}
