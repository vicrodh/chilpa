import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The player's "LCD": cover, times, title with its quality, who and from
// what, and the progress along its lower edge, which seeks.
Rectangle {
    id: lcd

    property bool showTimes: true
    readonly property bool qualityHovered: quality.hovered
    readonly property var castState: JSON.parse(Cast.snapshot || "{}")
    // The key of the page of what plays, when it is an entry of a source.
    readonly property string onAir: JSON.parse(Sources.snapshot || "{}").onAir || ""

    Connections {
        target: Player
        function onPathChanged() { Sources.refresh() }
    }

    height: Metrics.lcdHeight
    radius: Metrics.radiusLcd
    color: Theme.color.surfaceLcd

    ArtTile {
        id: cover
        objectName: "playerCover"
        anchors.left: parent.left
        anchors.leftMargin: Metrics.gap4
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.lcdArt
        height: Metrics.lcdArt
        radius: Metrics.radiusArtSmall
        source: Player.art
    }

    Text {
        id: elapsed
        anchors.left: cover.right
        anchors.leftMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        visible: lcd.showTimes && Player.playing
        text: Player.elapsed
        font.pointSize: Type.micro
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    Text {
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        visible: lcd.showTimes && Player.playing && !Player.live
        text: Player.remaining
        font.pointSize: Type.micro
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    Column {
        // The seek hit area overlaps the lower text edge; links keep priority.
        z: 1
        anchors.centerIn: parent
        width: parent.width - (cover.width + Metrics.gap16) * 2 - elapsed.implicitWidth * 2 // derived: room for the cover and the times on both sides
        spacing: Metrics.gap2

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Metrics.gap6

            Text {
                width: Math.min(implicitWidth, lcd.width / 2) // derived: at most half the LCD
                text: Player.title
                elide: Text.ElideRight
                font.pointSize: Type.row
                font.weight: Type.medium
                color: Theme.color.textPrimary
            }

            QualityIcon {
                objectName: "playbackIndicator"
                anchors.verticalCenter: parent.verticalCenter
                tier: Player.qualityTier
                glyph: Cast.active ? (lcd.castState.icon === "dlna" ? SourceGlyphs.dlna : SourceGlyphs.castAudio) : Icons.waveSquare
                iconFamily: Cast.active ? SourceGlyphs.font : ""
                color: Cast.active ? Theme.color.casting : tierColor
                visible: Cast.active || tier > 0

                HoverHandler { id: quality }
            }
        }

        Item {
            id: context
            width: parent.width
            height: Math.max(artist.implicitHeight, album.implicitHeight)

            Row {
                anchors.centerIn: parent
                spacing: Metrics.gap6

                CreditLine {
                    id: artist
                    width: Player.album === "" ? Math.min(implicitWidth, context.width) : Math.min(implicitWidth, (context.width - separator.implicitWidth - Metrics.gap12) / 2) // derived: share the available context row
                    credit: Player.artist
                    entry: lcd.onAir
                    font.pointSize: Type.small
                    color: hovered ? Theme.color.textPrimary : Theme.color.textSecondary
                }

                Text {
                    id: separator
                    visible: Player.artist !== "" && Player.album !== ""
                    text: "—"
                    font.pointSize: Type.small
                    color: Theme.color.textSecondary
                }

                Text {
                    id: album
                    objectName: "playerAlbum"
                    visible: Player.album !== ""
                    width: Player.artist === "" ? Math.min(implicitWidth, context.width) : Math.min(implicitWidth, (context.width - separator.implicitWidth - Metrics.gap12) / 2) // derived: share the available context row
                    text: Player.album
                    elide: Text.ElideRight
                    font.pointSize: Type.small
                    color: albumHover.hovered && Player.albumId > 0 ? Theme.color.textPrimary : Theme.color.textSecondary

                    HoverHandler {
                        id: albumHover
                        enabled: Player.albumId > 0
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        enabled: Player.albumId > 0
                        onTapped: Navigation.go("album", Player.albumId)
                    }
                }
            }
        }
    }

    SeekBar {
        objectName: "playerSeek"
        visible: !Player.live
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: lcd.radius
        anchors.rightMargin: lcd.radius
        position: Player.position
        duration: Player.duration
        bubbleText: Player.clock(hoverSeconds)
        onSeek: (seconds) => Player.seek(seconds)
    }
}
