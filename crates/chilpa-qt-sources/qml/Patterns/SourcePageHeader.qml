import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme/TextLinks.js" as TextLinks
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.kit
import com.blitzfc.chilpa.sources
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/qml/Components"

Column {
    id: header
    property var info: ({})
    property string tracksKey: ""
    property bool currentPage: false
    property bool contrasted: false
    property bool externalInformation: false
    signal information()
    signal artwork()
    signal selectToggle()
    signal context(real left, real top)
    property bool selecting: false
    property var record
    TrackListModel { id: tracks; kind: header.tracksKey ? "source" : ""; key: header.tracksKey; filterRev: Library.remoteRev }
    readonly property var sourceInfo: (JSON.parse(Sources.snapshot || "{}").panels || []).find(p => p.id === info.plugin) || ({name: info.plugin || ""})
    function follow(link) {
        if (/^(album|playlist):/.test(link)) {
            const id = Sources.referenceCard(info.plugin, link)
            if (id) Navigation.go("album", id)
        } else if (/^artist:/.test(link)) Navigation.goKey("source", info.plugin + " native:" + link)
        else if (/^https?:\/\//.test(link)) Qt.openUrlExternally(link)
    }
    spacing: Metrics.gap12
    property var playbackModel: tracks
    property double cardId: 0
    readonly property bool artist: info.kind === "artist"
    function favorite() { Sources.command("native", info.plugin, JSON.stringify({action:"favorite",ref:info.ref,on:!info.favorite})) }
    Item {
        width: parent.width
        implicitHeight: Math.max(portrait.height, body.height)
        ArtTile {
            id: portrait
            contain: header.info.art_fit === "contain"
            width: Math.min(Metrics.heroArt, parent.width / 3); height: width // derived: keep room for the text on narrow windows
            radius: header.artist ? width / 2 : Metrics.radiusCard // derived: circular artist portrait
            source: header.info.plugin && header.info.image ? Sources.art(header.info.plugin, header.info.image, Library.remoteArtRev) : header.info.art || ""
        }
        Icon {
            anchors.centerIn: portrait
            visible: header.artist && !portrait.ready
            glyph: SourceGlyphs.musicArtist; iconFamily: SourceGlyphs.font
            size: Metrics.iconPlay; color: Theme.color.textSecondary
        }
        HoverHandler { parent: portrait; cursorShape: Qt.PointingHandCursor }
        TapHandler { parent: portrait; onTapped: header.artwork() }
        Column {
            id: body; anchors.left: portrait.right; anchors.leftMargin: Metrics.heroGap; anchors.right: parent.right; spacing: Metrics.gap8
            Row {
                spacing: Metrics.gap6
                SourceIcon { sourceInfo: ({icon: header.sourceInfo.icon_image || header.sourceInfo.icon || ""}) }
                Text { text: header.sourceInfo.name || ""; textFormat: Text.PlainText; color: header.contrasted ? Theme.color.textOnAccent : Theme.color.textSecondary; font.pointSize: Type.meta }
            }
            Text { width: parent.width; text: header.info.title || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.display; font.weight: Type.bold; color: header.contrasted ? Theme.color.textOnAccent : Theme.color.textPrimary }
            AlbumArtistLine { width: parent.width; visible: header.cardId !== 0; albumId: header.cardId; contrasted: header.contrasted; color: header.contrasted ? Theme.color.textOnAccent : Theme.color.textSecondary }
            Text {
                visible: header.cardId === 0 && !!header.info.artist; text: header.info.artist || ""; textFormat: Text.PlainText
                color: header.contrasted ? Theme.color.textOnAccent : Theme.color.textSecondary; font.pointSize: Type.body
                HoverHandler { cursorShape: header.info.artist_ref ? Qt.PointingHandCursor : Qt.ArrowCursor }
                TapHandler { enabled: !!header.info.artist_ref; onTapped: header.follow(header.info.artist_ref) }
            }
            Text { width: parent.width; text: [header.info.release_date || "", header.playbackModel.summary || "", header.info.genre || ""].filter(Boolean).join(" · "); wrapMode: Text.Wrap; font.pointSize: Type.meta; color: header.contrasted ? Theme.color.textOnAccent : Theme.color.textSecondary }
            FontMetrics { id: descriptionFont; font: description.font }
            Text { id: description; width: parent.width; height: Math.min(implicitHeight, descriptionFont.height * 3); clip: true; visible: !!header.info.description; text: TextLinks.render(header.info.description, hoveredLink, color, header.contrasted ? color : Theme.color.accentHover); textFormat: Text.RichText; wrapMode: Text.WordWrap; maximumLineCount: 3; elide: Text.ElideRight; font.pointSize: Type.body; color: header.contrasted ? Theme.color.textOnAccent : Theme.color.textSecondary; onLinkActivated: link => header.follow(link) } // derived: cap the preview at three font lines
            PillButton { visible: description.implicitHeight > description.height; text: I18n.tr("Read more", I18n.rev); kind: header.contrasted ? "secondary" : "ghost"; onClicked: header.externalInformation ? header.information() : biography.open() }
            Row {
                visible: !!header.info.quality_label; spacing: Metrics.gap6
                QualityIcon { tier: header.info.quality_tier || 0; anchors.verticalCenter: parent.verticalCenter }
                Text { text: header.info.quality_label || ""; font.pointSize: Type.meta; color: header.contrasted ? Theme.color.textOnAccent : Theme.color.textSecondary }
            }
            Flow {
                width: parent.width; spacing: Metrics.gap8
                PillButton { visible: header.playbackModel.count > 0; text: I18n.tr("Play", I18n.rev); kind: "primary"; glyph: Icons.playerPlayFilled; filled: true; onClicked: header.playbackModel.playAll(false) }
                IconButton { visible: header.playbackModel.count > 0; round: true; tone: header.contrasted ? "overlay" : "secondary"; glyph: Icons.arrowsShuffle; tip: I18n.tr("Shuffle", I18n.rev); onClicked: header.playbackModel.playAll(true) }
                IconButton { objectName: "radioAction"; visible: !!header.info.radio_id; round: true; tone: header.contrasted ? "overlay" : "secondary"; glyph: Lucide.radio; iconFamily: Lucide.font; tip: header.artist ? I18n.tr("Artist radio", I18n.rev) : I18n.tr("Album radio", I18n.rev); onClicked: AlbumActions.play(header.info.radio_id, false) }
                IconButton { visible: !!header.info.ref && header.info.editable !== true; round: true; tone: header.contrasted ? "overlay" : "secondary"; glyph: Icons.heart; checked: header.info.favorite === true; tip: header.info.favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev); onClicked: header.favorite() }
                IconButton { objectName: "albumInformation"; round: true; tone: header.contrasted ? "overlay" : "secondary"; glyph: Icons.infoCircle; tip: I18n.tr("Information", I18n.rev); onClicked: header.externalInformation ? header.information() : biography.open() }
                IconButton { visible: !!header.record; round: true; tone: header.contrasted ? "overlay" : "secondary"; glyph: Icons.checkbox; checked: header.selecting; tip: header.selecting ? I18n.tr("Finish selecting", I18n.rev) : I18n.tr("Select tracks", I18n.rev); onClicked: header.selectToggle() }
                IconButton {
                    id: actions; visible: !!header.record; round: true; tone: header.contrasted ? "overlay" : "secondary"; glyph: Icons.dots; tip: I18n.tr("Album actions", I18n.rev)
                    onClicked: { const at = actions.mapToItem(header, actions.width - Metrics.menuWidth, actions.height + Metrics.gap4); header.context(at.x, at.y) }
                }
                Loader { visible: !!item && item.visible; active: !!header.record; sourceComponent: Component { AlbumVersionSelect { detail: header.record } } }
                MarkButtons { visible: header.cardId !== 0; kind: "album"; contrasted: header.contrasted; key: String(header.cardId) }
            }
        }
    }
    Modal {
        id: biography
        width: Math.min(Metrics.settingsMaxWidth, parent.width - Metrics.modalInsets)
        height: Math.min(parent.height - Metrics.modalInsets, Metrics.heroArt * 3) // derived: readable body with room for the title and close action
        Column {
            anchors.fill: parent; spacing: Metrics.gap12
            Text { id: biographyTitle; width: parent.width; text: header.info.title || ""; textFormat: Text.PlainText; font.pointSize: Type.titleSmall; color: Theme.color.textPrimary }
            ScrollView {
                width: parent.width; height: parent.height - biographyTitle.height - attribution.height - closeButton.height - parent.spacing * 3 // derived: the three gaps between four body items
                clip: true; contentWidth: availableWidth
                Text { width: parent.width; text: TextLinks.render(header.info.description, hoveredLink, color, Theme.color.accentHover); textFormat: Text.RichText; wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.body; onLinkActivated: link => { biography.close(); header.follow(link) } }
            }
            Text { id: attribution; width: parent.width; text: header.info.description_source || header.sourceInfo.name || ""; textFormat: Text.PlainText; font.pointSize: Type.meta; color: Theme.color.textTertiary }
            PillButton { id: closeButton; text: I18n.tr("Close", I18n.rev); onClicked: biography.close() }
        }
    }
}
