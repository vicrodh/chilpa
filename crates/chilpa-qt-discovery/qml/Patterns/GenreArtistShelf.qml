import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// The artists shelf of a genre's overview: one row of artist cards, a
// skeleton while it loads, and what it says when empty or failed. The cards
// come from the screen, which owns the artist card grid; they read
// `artists` and `shown` from their loader.
Item {
    id: shelf
    property string key: ""
    property string filter: ""
    property string order: "name"
    property Component cards
    AlbumGridModel { id:people; kind:"discoveryArtists"; key:shelf.key; filter:shelf.filter; order:shelf.order; columns:grid.item?.columns || 0; contentRev:Library.contentRev + Library.remoteRev }
    Loader { id:grid; anchors.fill:parent; sourceComponent:shelf.cards; readonly property var artists:people; readonly property bool shown:people.ready && people.error === "" }
    ShelfSkeleton { anchors.fill:parent; visible:!people.ready; round:true }
    Column {
        anchors.centerIn:parent; spacing:Metrics.gap12
        visible:people.ready && (people.error !== "" || people.albums === 0)
        Text { text:people.error || I18n.tr("No artists in this selection", I18n.rev); color:Theme.color.textSecondary; font.pointSize:Type.body }
        PillButton { anchors.horizontalCenter:parent.horizontalCenter; visible:people.error !== ""; text:I18n.tr("Retry", I18n.rev); onClicked:people.retry() }
    }
}
