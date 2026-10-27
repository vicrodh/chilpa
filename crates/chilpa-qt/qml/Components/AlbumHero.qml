import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// The head of a record's page: cover, kind and year, title, artist, the
// line of facts and the buttons (design/06 Album).
Item {
    id: hero

    property AlbumDetailModel detail
    property bool selecting: false
    property bool contrasted: false

    signal play()
    signal shuffle()
    signal selectToggle()
    signal context(real left, real top)
    signal artwork()
    // A right click on the cover, where the pointer is in the hero.
    signal artworkMenu(real left, real top)
    signal information()

    implicitHeight: Math.max(Metrics.heroArt, body.implicitHeight)

    ArtTile {
        id: cover
        width: Metrics.heroArt
        height: Metrics.heroArt
        source: hero.detail.art
    }
    HoverHandler { id: coverHover; parent: cover; cursorShape: Qt.PointingHandCursor }
    TapHandler { parent: cover; onTapped: hero.artwork() }
    TapHandler { parent: cover; acceptedButtons: Qt.RightButton; onTapped: (point) => { const at = cover.mapToItem(hero, point.position.x, point.position.y); hero.artworkMenu(at.x, at.y) } }

    Column {
        id: body
        anchors.left: cover.right
        anchors.leftMargin: Metrics.heroGap
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap6

        Text {
            text: hero.detail.kindLine
            font.pointSize: Type.micro
            font.weight: Type.semibold
            font.letterSpacing: Type.labelTracking
            color: hero.contrasted ? Theme.color.backdropText : Theme.color.textSecondary
            opacity: hero.contrasted ? 0.78 : 1
        }
        AdvisoryTitle {
            width: parent.width
            text: hero.detail.title
            explicitContent: hero.detail.explicitContent
            elide: Text.ElideRight
            font.pointSize: Type.display
            font.weight: Type.bold
            font.letterSpacing: Type.displayTracking
            color: hero.contrasted ? Theme.color.backdropText : Theme.color.textPrimary
        }
        AlbumArtistLine {
            width: parent.width; albumId: hero.detail.selectedId; contrasted: hero.contrasted
            document: JSON.parse(hero.detail.artistInformation || "{}")
            color: hero.contrasted ? Theme.color.backdropText : Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: hero.detail.meta
            elide: Text.ElideRight
            font.pointSize: Type.row
            color: hero.contrasted ? Theme.color.backdropText : Theme.color.textSecondary
            opacity: hero.contrasted ? 0.82 : 1
        }


        Flow {
            width: parent.width
            spacing: Metrics.gap8
            PillButton {
                kind: "primary"
                glyph: Icons.playerPlayFilled
                filled: true
                text: I18n.tr("Play", I18n.rev)
                onClicked: hero.play()
            }
            PillButton {
                kind: hero.contrasted ? "backdrop" : "secondary"
                glyph: Icons.arrowsShuffle
                text: I18n.tr("Shuffle", I18n.rev)
                onClicked: hero.shuffle()
            }
            IconButton { objectName: "albumInformation"; glyph: Icons.infoCircle; round: true; tone: hero.contrasted ? "overlay" : "secondary"; tip: I18n.tr("Information", I18n.rev); onClicked: hero.information() }
            MarkButtons {
                kind: "album"
                key: hero.detail.selectedId !== 0 ? hero.detail.selectedId.toString() : ""
                contrasted: hero.contrasted
            }
            IconButton {
                round: true
                glyph: Icons.checkbox
                checked: hero.selecting
                tone: hero.contrasted ? "overlay" : hero.selecting ? "accent" : "secondary"
                tip: hero.selecting ? I18n.tr("Finish selecting", I18n.rev)
                                    : I18n.tr("Select tracks", I18n.rev)
                onClicked: hero.selectToggle()
            }
            IconButton {
                id: contextButton
                round: true
                glyph: Icons.dots
                tone: hero.contrasted ? "overlay" : "secondary"
                tip: I18n.tr("Album actions", I18n.rev)
                onClicked: {
                    const at = contextButton.mapToItem(hero, contextButton.width - Metrics.menuWidth,
                                                       contextButton.height + Metrics.gap4)
                    hero.context(at.x, at.y)
                }
            }
            AlbumVersionSelect {
                detail: hero.detail
            }
        }
    }
}
