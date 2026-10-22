import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// A playlist of the library, headed as a source's playlist is
// (SourcePageHeader): its cover or a mosaic of four, where it lives, its
// name, what it holds and what it says of itself, then play, shuffle, the
// listener's marks, choosing tracks and its actions. Every playlist reads
// the same, whichever source keeps it.
Item {
    id: header
    property string name
    property string sourceName
    property url sourceIcon
    property var covers: []
    property string meta
    property string description
    property string markKey
    property bool canPlay: true
    property bool canSelect: false
    property bool selecting: false
    property bool contrasted: false
    signal play()
    signal shuffle()
    signal selectToggle()
    signal context(real left, real top)

    readonly property color ink: contrasted ? Theme.color.backdropText : Theme.color.textSecondary
    readonly property string tone: contrasted ? "overlay" : "secondary"
    property bool expanded: false
    implicitHeight: Math.max(cover.height, body.height)

    CoverCollage {
        id: cover
        objectName: "playlistHeaderCover"
        width: Math.min(Metrics.heroArt, header.width / 3); height: width // derived: keep room for the text on narrow windows
        covers: header.covers
        // With no cover yet, the empty tile a source's playlist shows.
        ArtTile { anchors.fill: parent; visible: header.covers.length === 0; radius: Metrics.radiusCard }
    }
    Column {
        id: body
        anchors.left: cover.right; anchors.leftMargin: Metrics.heroGap; anchors.right: parent.right
        spacing: Metrics.gap8
        Row {
            spacing: Metrics.gap6
            SourceIcon { visible: header.sourceIcon.toString() !== ""; sourceInfo: ({icon: header.sourceIcon.toString()}) }
            Text { text: header.sourceName; textFormat: Text.PlainText; color: header.ink; font.pointSize: Type.meta }
        }
        Text { width: parent.width; text: header.name; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.display; font.weight: Type.bold; color: header.contrasted ? Theme.color.backdropText : Theme.color.textPrimary }
        Text { width: parent.width; text: header.meta; wrapMode: Text.Wrap; font.pointSize: Type.meta; color: header.ink }
        FontMetrics { id: descriptionFont; font: description.font }
        Text {
            id: description
            objectName: "playlistHeaderDescription"
            width: parent.width
            height: header.expanded ? implicitHeight : Math.min(implicitHeight, descriptionFont.height * 3) // derived: three lines until it is opened
            clip: true
            visible: header.description !== ""
            text: header.description; textFormat: Text.PlainText; wrapMode: Text.WordWrap
            font.pointSize: Type.body; color: header.ink
        }
        PillButton { visible: !header.expanded && description.implicitHeight > description.height; text: I18n.tr("Read more", I18n.rev); kind: header.contrasted ? "backdrop" : "ghost"; onClicked: header.expanded = true }
        Flow {
            width: parent.width; spacing: Metrics.gap8
            PillButton { objectName: "playlistPlay"; visible: header.canPlay; text: I18n.tr("Play", I18n.rev); kind: "primary"; glyph: Icons.playerPlayFilled; filled: true; onClicked: header.play() }
            IconButton { objectName: "playlistShuffle"; visible: header.canPlay; round: true; tone: header.tone; glyph: Icons.arrowsShuffle; tip: I18n.tr("Shuffle", I18n.rev); onClicked: header.shuffle() }
            MarkButtons { visible: header.markKey !== ""; kind: "playlist"; key: header.markKey; contrasted: header.contrasted }
            IconButton { visible: header.canSelect; round: true; tone: header.tone; glyph: Icons.checkbox; checked: header.selecting; tip: header.selecting ? I18n.tr("Finish selecting", I18n.rev) : I18n.tr("Select tracks", I18n.rev); onClicked: header.selectToggle() }
            IconButton {
                id: actions; objectName: "playlistActions"; round: true; tone: header.tone; glyph: Icons.dots; tip: I18n.tr("Queue and playlist actions", I18n.rev)
                onClicked: { const at = actions.mapToItem(header, actions.width - Metrics.menuWidth, actions.height + Metrics.gap4); header.context(at.x, at.y) }
            }
        }
    }
}
