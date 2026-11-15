import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

Button {
    id: card
    property var entry: ({})
    property string detail: ""
    property bool activeImages: true
    signal openAlbum(int album)
    readonly property bool recording: entry.kind === "recording"
    readonly property bool genre: entry.kind === "genre"
    readonly property bool composer: entry.kind === "composer"
    readonly property string genreImage: entry.portrait || (entry.art?.key ? "qrc:/qt/qml/app/chilpa/discovery/assets/genres/" + entry.art.key + ".jpg" : (entry.images || [])[0] || "")
    function initials() { return (entry.name || "").split(/\s+/).filter(Boolean).slice(0, 2).map(word => word[0]).join("").toUpperCase() }
    implicitHeight: genre || composer ? Metrics.sceneCardHeight : Metrics.discoveryEntryHeight
    padding: genre || composer ? 0 : Metrics.gap16
    hoverEnabled: true
    Accessible.name: entry.name || ""
    Accessible.description: detail
    Keys.onReturnPressed: clicked()
    Keys.onEnterPressed: clicked()
    background: Rectangle {
        radius: Metrics.radiusCard
        color: card.down ? Theme.color.stateSelected : card.hovered ? Theme.color.stateHover : Theme.color.surfacePanel
        Edge {}
        FocusRing { visible: card.activeFocus }
    }
    contentItem: Item {
        Loader {
            id: portrait
            width: Math.min(parent.width - Metrics.gap24, parent.height - Metrics.cardTextHeight - Metrics.cardTextGap)
            height: width; anchors.top: parent.top; anchors.topMargin: Metrics.gap12; anchors.horizontalCenter: parent.horizontalCenter
            active: card.genre || card.composer
            visible: active
            sourceComponent: card.genre ? genrePortrait : composerPortrait
        }
        Component {
            id: genrePortrait
            ArtTile {
                id: genreArt
                objectName: "discoveryGenrePortrait"
                radius: width / 2 // derived: the artist card's circular image
                source: card.activeImages ? card.genreImage : ""
                Text { anchors.centerIn: parent; visible: !genreArt.ready; text: card.initials(); color: Theme.color.textSecondary; font.pointSize: Type.section }
                HoverHandler {
                    onHoveredChanged: hovered && card.entry.art?.key && !card.entry.portrait
                        ? TipState.show([card.entry.art.credit,card.entry.art.license,card.entry.art.licenseUrl,card.entry.art.page].filter(Boolean).join("\n"), genreArt) : TipState.hide(genreArt)
                }
                onSourceChanged: if (source.toString() === "") TipState.hide(genreArt)
                Component.onDestruction: TipState.hide(genreArt)
            }
        }
        Component {
            id: composerPortrait
            ArtistPortrait {
                id: composerArt
                radius: width / 2 // derived: a circular portrait
                chosen: card.entry.portrait || ""; name: card.entry.name || ""
                activeImages: card.activeImages
                Text { anchors.centerIn: parent; visible: !composerArt.ready; text: card.initials(); color: Theme.color.textSecondary; font.pointSize: Type.section }
            }
        }
        Column {
            anchors.left: parent.left; anchors.right: parent.right; anchors.top: portrait.bottom
            anchors.topMargin: Metrics.cardTextGap; anchors.margins: Metrics.gap8
            visible: card.genre || card.composer; spacing: Metrics.gap4
            Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: card.entry.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary }
            Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; text: card.detail; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        }
        Icon {
            id: icon
            anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
            glyph: card.entry.kind === "genre" ? Icons.hierarchy2 : card.entry.kind === "composer" ? Icons.pencil : card.recording ? Icons.disc : Icons.music
            size: Metrics.iconMd; color: Theme.color.accent
            visible: !card.genre && !card.composer
        }
        Column {
            anchors.left: icon.right; anchors.leftMargin: Metrics.gap12
            anchors.right: arrow.left; anchors.rightMargin: Metrics.gap8
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap4
            visible: !card.genre && !card.composer
            Text { width: parent.width; text: card.entry.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.body; font.weight: Type.semibold; color: Theme.color.textPrimary }
            Text { width: parent.width; text: card.detail; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        }
        Icon { id: arrow; visible: !card.genre && !card.composer; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; glyph: Icons.chevronRight; size: Metrics.iconSm; color: Theme.color.textTertiary }
    }
}
