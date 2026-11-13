import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"

// One release a search found (MusicBrainz or Discogs): its title and artist,
// then what tells two pressings apart -- date, country, label, catalog
// number, format and how many tracks. `hit` is one entry of the editor's
// view.
ListRowSurface {
    id: root

    property var hit: ({})
    property bool chosen: false
    property bool showScore: false

    signal choose()

    readonly property string detail: [hit.date, hit.country, hit.label, hit.catalog_number, hit.format,
        hit.tracks > 0 ? I18n.trCount("{} track", "{} tracks", hit.tracks, I18n.rev) : "",
        showScore ? I18n.trArgs("Match score: {}",[String(hit.score || 0)],I18n.rev) : ""]
        .filter(part => part !== undefined && part !== "").join(" · ")

    implicitHeight: Metrics.metadataHit
    selected: chosen
    hovered: pointer.hovered
    activeFocusOnTab:true
    Accessible.role:Accessible.Button
    Accessible.name:(hit.title || "") + " · " + (hit.artist || "")
    Accessible.description:detail
    Keys.onReturnPressed:choose()
    Keys.onEnterPressed:choose()
    Keys.onSpacePressed:choose()
    FocusRing { visible:root.activeFocus }

    HoverHandler { id: pointer }
    HoverHandler { cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: root.choose() }

    Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Metrics.gap8
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            objectName:"metadataReleaseHitTitle"
            width: parent.width
            text: root.hit.artist ? root.hit.title + " — " + root.hit.artist : (root.hit.title || "")
            textFormat:Text.PlainText
            elide: Text.ElideRight
            font.pointSize: Type.row
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: root.detail
            textFormat:Text.PlainText
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }
}
