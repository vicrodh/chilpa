import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme/TextLinks.js" as TextLinks
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.artwork
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/artwork/qml/Components"
import "../Components"

// The top of an artist's page, as QBZ lays it out: a round portrait, the
// name, what the artist does, a biography cut at 360 characters with
// "Read more", and a row of round actions: Follow, Radio (and the mix when
// the source makes one), Network, and the rest under three dots.
Item {
    id: header
    property var info: ({})
    property bool local: false
    property bool networkOpen: false
    // Showing what the listener keeps of the artist in its source.
    property bool inLibrary: false
    signal toggleNetwork()
    signal libraryChosen(bool on)
    readonly property var sourceInfo: (JSON.parse(Sources.snapshot || "{}").panels || []).find(p => p.id === info.plugin) || ({name: info.plugin || ""})
    readonly property string bio: info.description || ""
    // At most 360 characters, cut at the last space, with an ellipsis.
    readonly property string bioShort: {
        if (bio.length <= Metrics.bioChars) return bio
        const cut = bio.slice(0, Metrics.bioChars)
        const at = cut.lastIndexOf(" ")
        return (at > 0 ? cut.slice(0, at) : cut) + "…"
    }
    readonly property bool favorite: local ? Ratings.favorite("artist", info.title || "", Ratings.rev) : info.favorite === true
    function follow(link) {
        if (/^artist:/.test(link)) Navigation.goKey("source", info.plugin + " native:" + link)
        else if (/^https?:\/\//.test(link)) Qt.openUrlExternally(link)
    }
    implicitHeight: Math.max(portrait.height, body.height)

    ArtTile {
        id: portrait
        width: Math.min(Metrics.artistPortrait, header.width / 3); height: width // derived: room for the text on narrow windows
        radius: width / 2 // derived: a circle
        // The picture the listener chose for the artist wins over the one
        // the source or the records bring.
        readonly property url chosen: Artwork.artist(header.info.title || "", Artwork.rev)
        source: chosen.toString() !== "" ? chosen : header.info.plugin && header.info.image ? Sources.art(header.info.plugin, header.info.image, Library.remoteArtRev) : header.info.art || ""
    }
    // As QBZ's portrait: a click shows it whole, a right click its menu.
    HoverHandler { parent: portrait; cursorShape: portrait.ready ? Qt.PointingHandCursor : Qt.ArrowCursor }
    TapHandler { parent: portrait; onTapped: if (portrait.ready) lightbox.show(portrait.source) }
    TapHandler {
        parent: portrait
        acceptedButtons: Qt.RightButton
        onTapped: (point) => {
            const at = portrait.mapToItem(header, point.position.x, point.position.y)
            portraitMenu.custom = portrait.chosen.toString() !== ""
            portraitMenu.showing = portrait.ready
            portraitMenu.address = /^https?:\/\//.test(header.info.image || "") ? header.info.image : ""
            portraitMenu.popupAt(at.x, at.y)
        }
    }
    CoverMenu {
        id: portraitMenu
        kind: "artist"
        onChoose: Artwork.chooseArtist(header.info.title || "")
        onForget: Artwork.forgetArtist(header.info.title || "")
        onView: lightbox.show(portrait.source)
        onSave: Artwork.saveArtist(header.info.title || "", portrait.source)
    }
    CoverLightbox { id: lightbox }
    Icon {
        anchors.centerIn: portrait
        visible: !portrait.ready
        glyph: SourceGlyphs.musicArtist; iconFamily: SourceGlyphs.font
        size: Metrics.iconPlay; color: Theme.color.textSecondary
    }
    // At the top right, level with the source's name.
    ArtistSources {
        anchors.right: parent.right
        y: body.y
        z: 1
        source: header.local ? "" : header.info.plugin || ""
        artist: header.info.title || ""
        inLibrary: header.inLibrary
        sources: (JSON.parse(Sources.snapshot || "{}").panels || []).filter(p => p.artists === true)
        onLibraryChosen: (on) => header.libraryChosen(on)
    }
    Column {
        id: body
        anchors.left: portrait.right; anchors.leftMargin: Metrics.blockGap; anchors.right: parent.right
        anchors.verticalCenter: portrait.verticalCenter
        spacing: Metrics.gap8
        Row {
            visible: !header.local; spacing: Metrics.gap6
            SourceIcon { sourceInfo: ({icon: header.sourceInfo.icon_image || header.sourceInfo.icon || ""}) }
            Text { text: header.sourceInfo.name || ""; textFormat: Text.PlainText; color: Theme.color.textSecondary; font.pointSize: Type.meta }
        }
        Text { width: parent.width; text: header.info.title || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.display; font.weight: Type.bold; color: Theme.color.textPrimary }
        // What the artist does, when the source says: composer, producer...
        Text { width: parent.width; visible: text !== ""; text: (header.info.roles || []).join(" · "); textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        Text {
            id: description
            width: parent.width; visible: header.bio !== ""
            text: TextLinks.render(header.bioShort, hoveredLink, color, Theme.color.accentHover); textFormat: Text.RichText; wrapMode: Text.WordWrap
            font.pointSize: Type.meta; color: Theme.color.textSecondary
            onLinkActivated: (link) => header.follow(link)
        }
        Text {
            visible: header.bioShort !== header.bio
            text: I18n.tr("Read more", I18n.rev); font.pointSize: Type.meta; color: hover.hovered ? Theme.color.accentHover : Theme.color.accent
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: biography.open() }
        }
        Flow {
            width: parent.width; spacing: Metrics.gap12
            IconButton {
                objectName: "followAction"; round: true; size: Metrics.button32; glyph: header.favorite ? Icons.heartFilled : Icons.heart; filled: header.favorite; checked: header.favorite
                tip: header.favorite ? I18n.tr("Unfollow", I18n.rev) : I18n.tr("Follow", I18n.rev)
                onClicked: header.local ? Ratings.setFavorite("artist", header.info.title || "", !header.favorite)
                    : Sources.command("native", header.info.plugin, JSON.stringify({action:"favorite",ref:header.info.ref,on:!header.favorite}))
            }
            IconButton {
                id: radio; objectName: "radioAction"; visible: !!header.info.radio_id || !!header.info.mix_id; round: true; size: Metrics.button32
                glyph: Lucide.radio; iconFamily: Lucide.font; tip: I18n.tr("Artist radio", I18n.rev)
                // One source makes a radio and a mix: the button offers both.
                onClicked: header.info.mix_id && header.info.radio_id ? radioMenu.open() : AlbumActions.play(header.info.radio_id || header.info.mix_id, false)
                Menu {
                    id: radioMenu; y: parent.height + Metrics.gap4
                    entries: [{ text: I18n.tr("Artist radio", I18n.rev) }, { text: I18n.tr("Artist mix", I18n.rev) }]
                    onChosen: (index) => AlbumActions.play(index === 0 ? header.info.radio_id : header.info.mix_id, false)
                }
            }
            IconButton {
                objectName: "networkAction"; round: true; size: Metrics.button32; glyph: SourceGlyphs.spiderWeb; iconFamily: SourceGlyphs.font; checked: header.networkOpen
                tip: I18n.tr("Network", I18n.rev); onClicked: header.toggleNetwork()
            }
            // The artist's own links (site, social), when the source has
            // them. Artist Scene waits for the scene to exist.
            IconButton {
                id: more; objectName: "moreAction"; visible: (header.info.links || []).length > 0; round: true; size: Metrics.button32; glyph: Icons.dots; tip: I18n.tr("Links", I18n.rev)
                onClicked: { moreMenu.entries = (header.info.links || []).map(l => ({ text: l.title || l.url, url: l.url })); moreMenu.open() }
                Menu {
                    id: moreMenu; y: parent.height + Metrics.gap4
                    onChosen: (index) => { const url = entries[index].url; if (url) Qt.openUrlExternally(url) }
                }
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
                ScrollBar.vertical: ViewportScrollBar {}
                Text { width: parent.width; text: TextLinks.render(header.bio, hoveredLink, color, Theme.color.accentHover); textFormat: Text.RichText; wrapMode: Text.WordWrap; color: Theme.color.textSecondary; font.pointSize: Type.body; onLinkActivated: (link) => header.follow(link) }
            }
            Text {
                id: attribution; width: parent.width; textFormat: Text.PlainText; font.pointSize: Type.meta; color: Theme.color.textTertiary
                text: [I18n.tr("Source: {}", I18n.rev).replace("{}", header.info.description_source || header.sourceInfo.name || ""), header.info.bio_updated ? I18n.tr("Updated {}", I18n.rev).replace("{}", header.info.bio_updated) : ""].filter(Boolean).join(" · ")
            }
            PillButton { id: closeButton; text: I18n.tr("Close", I18n.rev); onClicked: biography.close() }
        }
    }
}
