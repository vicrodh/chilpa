import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.artwork
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// A composer's page opens as an artist's: a round portrait shown whole on
// a click, with its picture menu on a right click; the name; the years and
// period the composer table knows; how many works the library holds; and
// the way to the artist's own page.
Item {
    id: header
    objectName: "composerHeader"
    property string name: ""
    // {years, period} from the composer table, or null for a composer it does not know.
    property var about: null
    property int works: 0
    property bool activeImages: visible
    readonly property string facts: [about ? about.years : "", about ? about.period : "",
        works > 0 ? I18n.trCount("{} composition", "{} compositions", works, I18n.rev) : ""].filter(Boolean).join(" · ")
    implicitHeight: Math.max(portrait.height, body.height)
    ArtistPortrait {
        id: portrait
        objectName: "composerPortrait"
        width: Math.min(Metrics.artistPortrait, header.width / 3); height: width // derived: room for the text on narrow windows
        radius: width / 2 // derived: a circle
        // A picture the listener chose is shown whole, not cropped to the circle.
        readonly property url custom: Artwork.artist(header.name, Artwork.rev)
        contain: custom.toString() !== ""
        chosen: custom
        name: header.name
        activeImages: header.activeImages
        Text {
            anchors.centerIn: parent
            visible: !portrait.ready
            text: header.name.split(/\s+/).filter(Boolean).slice(0, 2).map(word => word[0]).join("").toUpperCase()
            color: Theme.color.textSecondary; font.pointSize: Type.title
        }
    }
    HoverHandler { parent: portrait; cursorShape: portrait.ready ? Qt.PointingHandCursor : Qt.ArrowCursor }
    TapHandler { parent: portrait; onTapped: if (portrait.ready) pictureMenu.view(portrait.source) }
    TapHandler {
        parent: portrait
        acceptedButtons: Qt.RightButton
        onTapped: point => {
            const at = portrait.mapToItem(header, point.position.x, point.position.y)
            pictureMenu.popupAt(header.name, portrait.ready ? portrait.source : "", at.x, at.y)
        }
    }
    ComposerPictureMenu { id: pictureMenu; anchors.fill: parent }
    Column {
        id: body
        anchors.left: portrait.right; anchors.leftMargin: Metrics.blockGap; anchors.right: parent.right
        anchors.verticalCenter: portrait.verticalCenter
        spacing: Metrics.gap8
        Text { width: parent.width; text: I18n.tr("Composer", I18n.rev); textFormat: Text.PlainText; color: Theme.color.textSecondary; font.pointSize: Type.meta }
        Text { width: parent.width; text: header.name; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.display; font.weight: Type.bold; color: Theme.color.textPrimary }
        Text { width: parent.width; visible: text !== ""; text: header.facts; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.meta; color: Theme.color.textSecondary }
        PillButton {
            objectName: "composerAbout"
            text: I18n.tr("About this composer", I18n.rev); kind: "ghost"
            onClicked: {
                Visit.keep("artist.aboutIntent", {name:header.name, at:Date.now()})
                Navigation.openArtist(header.name)
            }
        }
    }
}
