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
    implicitHeight: genre ? Metrics.discoveryGenreHeight : composer ? Metrics.sceneCardHeight : Metrics.discoveryEntryHeight
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
        DiscoveryMosaic {
            anchors.fill: parent
            visible: card.genre
            images: card.genre ? card.entry.images || [] : []
            art: card.entry.art || ({})
            label: card.entry.name || ""; activeImages: card.activeImages
        }
        Rectangle {
            anchors.fill: parent
            visible: card.genre
            radius: Metrics.radiusCard
            gradient: Gradient {
                GradientStop { position: 0; color: "transparent" }
                GradientStop { position: 1; color: Theme.color.overlayScrim }
            }
        }
        Column {
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: Metrics.gap16
            spacing: Metrics.gap4; visible: card.genre
            Text { width: parent.width; text: card.entry.name || ""; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.backdropText }
            Text { width: parent.width; text: card.detail; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.backdropTextMuted }
        }
        ArtistPortrait {
            id: portrait
            width: Math.min(parent.width - Metrics.gap24, parent.height - Metrics.cardTextHeight - Metrics.cardTextGap)
            height: width; anchors.top: parent.top; anchors.topMargin: Metrics.gap12; anchors.horizontalCenter: parent.horizontalCenter
            radius: width / 2 // derived: a circular portrait
            visible: card.composer
            chosen: card.entry.portrait || ""
            name: card.composer ? card.entry.name || "" : ""
            activeImages: card.composer && card.activeImages
            Text {
                anchors.centerIn: parent; visible: !portrait.ready
                text: (card.entry.name || "").split(/\s+/).filter(Boolean).slice(0, 2).map(word => word[0]).join("").toUpperCase()
                color: Theme.color.textSecondary; font.pointSize: Type.section
            }
        }
        Column {
            anchors.left: parent.left; anchors.right: parent.right; anchors.top: portrait.bottom
            anchors.topMargin: Metrics.cardTextGap; anchors.margins: Metrics.gap8
            visible: card.composer; spacing: Metrics.gap4
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
