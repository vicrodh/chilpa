import QtQuick
import app.chilpa.kit
import app.chilpa.artwork
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// Two bounded previews live only while the artwork selector is open.
Modal {
    id: editor
    objectName: "genreArtworkEditor"
    property string key: ""
    readonly property url cardImage: opened ? Artwork.genre(key, Artwork.rev) : ""
    readonly property url headerImage: opened ? Artwork.genreHeader(key, Artwork.rev) : ""
    Column {
        width: parent.width; spacing: Metrics.gap24
        Text { width: parent.width; text: I18n.tr("Genre images", I18n.rev); wrapMode: Text.WordWrap; font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary }
        Repeater {
            model: ["card", "header"]
            Column {
                id: field
                required property string modelData
                readonly property bool card: modelData === "card"
                readonly property url image: card ? editor.cardImage : editor.headerImage
                readonly property bool chosen: image.toString() !== ""
                width: parent.width; spacing: Metrics.gap8
                Text { text: field.card ? I18n.tr("Card image", I18n.rev) : I18n.tr("Header image", I18n.rev); font.pointSize: Type.body; color: Theme.color.textPrimary }
                Row {
                    width: parent.width; spacing: Metrics.gap16
                    Item {
                        width: Math.min(parent.width / 2, Metrics.sceneCardWidth); height: width // derived: one half of the selector row
                        ArtTile {
                            objectName: field.card ? "genreCardPreview" : "genreHeaderPreview"
                            anchors.centerIn: parent
                            width: parent.width; height: field.card ? width : width / 4 // derived: square card or 4:1 banner
                            radius: field.card ? width / 2 : Metrics.radiusCard // derived: circular card preview
                            source: field.image; contain: !field.card
                            Text { anchors.centerIn: parent; visible: !field.chosen; text: I18n.tr("Default", I18n.rev); font.pointSize: Type.meta; color: Theme.color.textSecondary }
                        }
                    }
                    Column {
                        anchors.verticalCenter: parent.verticalCenter; spacing: Metrics.gap8
                        PillButton {
                            objectName: field.card ? "genreCardChoose" : "genreHeaderChoose"
                            text: field.chosen ? I18n.tr("Change image", I18n.rev) : I18n.tr("Add image", I18n.rev)
                            onClicked: field.card ? Artwork.chooseGenre(editor.key) : Artwork.chooseGenreHeader(editor.key)
                        }
                        PillButton {
                            objectName: field.card ? "genreCardForget" : "genreHeaderForget"
                            text: I18n.tr("Remove image", I18n.rev); visible: field.chosen
                            onClicked: field.card ? Artwork.forgetGenre(editor.key) : Artwork.forgetGenreHeader(editor.key)
                        }
                    }
                }
            }
        }
        PillButton { anchors.right: parent.right; text: I18n.tr("Close", I18n.rev); onClicked: editor.close() }
    }
}
