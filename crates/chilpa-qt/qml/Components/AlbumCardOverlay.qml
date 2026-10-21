import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

// What a card shows under the pointer (design/03 AlbumCard): the scrim, the
// check that selects it, its favorite, play, its quick view and its menu.
// Only one exists at a time, so it can afford what a card cannot.
Rectangle {
    id: overlay

    property int albumId
    property string year
    property bool selected
    property bool expanded
    // Only where a grid can open one under the row (ADR-0007).
    property bool canExpand: true
    readonly property var nativeInfo: albumId < 0 ? JSON.parse(Sources.card(albumId, Library.remoteRev) || "{}") : ({})
    readonly property bool favoriteSupported: albumId > 0 || !!nativeInfo.plugin && /^(album|artist|label|playlist):/.test(nativeInfo.ref || "")
    readonly property bool favorite: nativeInfo.plugin ? nativeInfo.favorite === true : Ratings.favorite("album", albumId, Ratings.rev)
    readonly property var information: JSON.parse(AlbumActions.information(albumId, Library.contentRev + Library.remoteRev) || "{}")
    readonly property string genre: (Array.isArray(information.genres) && typeof information.genres[0] === "string"
        ? information.genres[0] : information.genre || "").split(/[,;]+/)[0].trim()
    // "April 26, 1981" when the record's files or its source say the day.
    readonly property string released: information.released || year

    signal select()
    signal quick()
    signal menu(real x, real y)

    radius: Metrics.radiusCard
    color: Theme.color.overlayScrim

    Column {
        id: metadata
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.right: selector.left
        anchors.leftMargin: Metrics.overlayPad
        anchors.topMargin: Metrics.overlayPad
        anchors.rightMargin: Metrics.gap6
        spacing: Metrics.gap4
        Text {
            visible: overlay.released !== ""
            width: parent.width
            elide: Text.ElideRight
            text: overlay.released
            font.pointSize: Type.meta
            font.weight: Type.semibold
            color: Theme.color.backdropText
        }
        Text {
            width: parent.width
            visible: overlay.genre !== ""
            text: overlay.genre
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.backdropText
        }
    }

    Item {
        id: selector
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Metrics.overlayPad
        width: Metrics.overlayCheck
        height: Metrics.overlayCheck

        Icon {
            anchors.centerIn: parent
            glyph: overlay.selected ? Icons.circleCheckFilled : Icons.circle
            filled: overlay.selected
            size: Metrics.overlayCheckIcon
            color: Theme.color.backdropText
        }
        MouseArea {
            cursorShape: Qt.PointingHandCursor
            anchors.fill: parent
            hoverEnabled: true
            onClicked: overlay.select()
            onContainsMouseChanged: containsMouse ? TipState.show(I18n.tr("Select", I18n.rev), selector) : TipState.hide(selector)
        }
    }

    Item {
        id: actions
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: Metrics.overlayPad
        anchors.rightMargin: Metrics.overlayPad
        anchors.bottomMargin: Metrics.overlayPad + Metrics.iconSm + Metrics.gap4 + Metrics.gap6
        height: Metrics.overlayPlay

        Rectangle {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: Metrics.overlayPlay
            height: Metrics.overlayPlay
            radius: width / 2 // derived: a circle
            color: button.containsMouse ? Theme.color.accentHover : Theme.color.accent

            Icon {
                anchors.centerIn: parent
                glyph: Icons.playerPlayFilled
                filled: true
                size: Metrics.iconNav
                color: Theme.color.textOnAccent
            }
            MouseArea {
                id: button
                cursorShape: Qt.PointingHandCursor
                anchors.fill: parent
                hoverEnabled: true
                onClicked: AlbumActions.play(overlay.albumId, false)
                onContainsMouseChanged: containsMouse ? TipState.show(I18n.tr("Play", I18n.rev), parent) : TipState.hide(parent)
            }
        }
        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap6

            IconButton {
                visible: overlay.canExpand
                size: Metrics.overlayButton
                round: true
                tone: "overlay"
                checked: overlay.expanded
                glyph: Icons.layoutBottombarExpand
                tip: I18n.tr("Quick view", I18n.rev)
                onClicked: overlay.quick()
            }
            FavoriteButton {
                // A source's record is a favorite of the service, not of the library.
                objectName: "albumFavorite"
                visible: overlay.favoriteSupported
                size: Metrics.overlayButton
                iconSize: Metrics.overlayIcon
                round: true
                tone: "overlay"
                checked: overlay.favorite
                heart: true
                busy: !!overlay.nativeInfo.plugin && Sources.favoritePending(overlay.nativeInfo.plugin, overlay.nativeInfo.ref || "", Sources.revision)
                glyph: overlay.favorite ? Icons.heartFilled : Icons.heart
                filled: overlay.favorite
                tip: overlay.favorite ? I18n.tr("Remove from favorites", I18n.rev) : I18n.tr("Add to favorites", I18n.rev)
                onClicked: {
                    if (overlay.nativeInfo.plugin)
                        Sources.command("native", overlay.nativeInfo.plugin, JSON.stringify({action:"favorite",ref:overlay.nativeInfo.ref,on:!overlay.favorite}))
                    else
                        Ratings.setFavorite("album", overlay.albumId, !overlay.favorite)
                }
            }
            IconButton {
                id: more
                size: Metrics.overlayButton
                round: true
                tone: "overlay"
                glyph: Icons.dots
                tip: I18n.tr("More", I18n.rev)
                onClicked: {
                    const at = more.mapToItem(overlay, 0, 0)
                    overlay.menu(at.x, at.y + more.height)
                }
            }
        }
    }
}
