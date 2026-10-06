import QtQuick
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"

// What the place a source's listing stands in is: a show above its
// episodes, with its picture, whose it is, what it is about and what its
// plugin knows of it. Nothing at all when the place has nothing to say.
Item {
    id: about

    // { title, subtitle, image, description, facts: [{ label, value }] }, or null.
    property var entry: null
    readonly property bool shown: !!entry && (entry.image !== "" || entry.description !== "")

    visible: shown
    implicitHeight: shown ? Metrics.pageHero + Metrics.gap12 : 0 // derived: the picture and the gap under it

    ArtTile {
        id: picture
        width: Metrics.pageHero
        height: Metrics.pageHero
        source: about.entry ? about.entry.image : ""
    }

    Column {
        anchors.left: picture.right
        anchors.leftMargin: Metrics.heroGap
        anchors.right: parent.right
        anchors.verticalCenter: picture.verticalCenter
        spacing: Metrics.gap6

        Text {
            width: parent.width
            visible: text !== ""
            text: about.entry ? about.entry.subtitle : ""
            elide: Text.ElideRight
            font.pointSize: Type.row
            font.weight: Type.medium
            color: Theme.color.textPrimary
        }
        Text {
            objectName: "aboutDescription"
            width: parent.width
            visible: text !== ""
            text: about.entry ? about.entry.description : ""
            wrapMode: Text.WordWrap
            maximumLineCount: 5
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
        Text {
            width: parent.width
            visible: text !== ""
            // The facts that are not links, on one line.
            text: about.entry ? (about.entry.facts || []).filter(f => !f.url).map(f => f.label + ": " + f.value).join("  ·  ") : ""
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
    }
}
