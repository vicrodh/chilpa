import QtQuick
import com.blitzfc.chilpa
import "../Primitives"
import "../Theme"

// One release a search found (MusicBrainz or Discogs): its title and artist,
// then what tells two pressings apart -- date, country, label, catalog
// number, format and how many tracks. `hit` is one entry of the editor's
// view.
ListRowSurface {
    id: root

    property var hit: ({})
    property bool chosen: false

    signal choose()

    readonly property string detail: [hit.date, hit.country, hit.label, hit.catalog_number, hit.format,
        hit.tracks > 0 ? I18n.trCount("{} track", "{} tracks", hit.tracks, I18n.rev) : ""]
        .filter(part => part !== undefined && part !== "").join(" · ")

    implicitHeight: Metrics.metadataHit
    selected: chosen
    hovered: pointer.hovered

    HoverHandler { id: pointer }
    TapHandler { onTapped: root.choose() }

    Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Metrics.gap8
        anchors.rightMargin: Metrics.gap8
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: root.hit.artist ? root.hit.title + " — " + root.hit.artist : (root.hit.title || "")
            elide: Text.ElideRight
            font.pointSize: Type.row
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: root.detail
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }
}
