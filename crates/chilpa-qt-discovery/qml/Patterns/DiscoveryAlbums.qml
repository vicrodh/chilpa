import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/qml/Components"

Item {
    id: section
    property string key: ""
    property string filter: ""
    property string order: "title"
    readonly property alias scrollView: grid.scrollView
    readonly property bool ready: records.ready
    readonly property string error: records.error
    function retry() { records.retry() }
    AlbumGridModel { id:records; objectName:"discoveryAlbumModel"; kind:"discovery"; key:section.key; filter:section.filter; order:section.order; columns:grid.columns; contentRev:Library.contentRev + Library.remoteRev }
    AlbumGrid { id:grid; anchors.fill:parent; model:records; onOpen:albumId => Navigation.go("album",albumId) }
    Text { anchors.centerIn:parent; visible:records.ready && records.albums === 0; text:I18n.tr("No albums in this selection",I18n.rev); font.pointSize:Type.body; color:Theme.color.textSecondary }
}
