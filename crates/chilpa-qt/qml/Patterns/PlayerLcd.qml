import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// The player's "LCD": cover, times, title with its quality, who and from
// what, and the progress along its lower edge, which seeks.
Rectangle {
    id: lcd

    property bool showTimes: true
    // The bar standing tall with the waveform: the times go beside it,
    // not here (the owner, 2026-10-09).
    property bool tall: false
    readonly property bool qualityHovered: quality.hovered
    readonly property var castState: JSON.parse(Cast.snapshot || "{}")
    // The key of the page of what plays, when it is an entry of a source.
    readonly property string parentSource: JSON.parse(Sources.snapshot || "{}").playing_parent || ""
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
        visible: lcd.showTimes && !lcd.tall && Player.playing
        text: Player.elapsed
        font.pointSize: Type.micro
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    Text {
        id: remaining
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap12
        anchors.verticalCenter: parent.verticalCenter
        visible: lcd.showTimes && !lcd.tall && Player.playing && !Player.live
        text: Player.remaining
        font.pointSize: Type.micro
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    Column {
        id: labels
        // The seek hit area overlaps the lower text edge; links keep priority.
        z: 1
        anchors.verticalCenter: parent.verticalCenter
        anchors.left: elapsed.visible ? elapsed.right : cover.right
        anchors.leftMargin: Metrics.gap12
        anchors.right: remaining.visible ? remaining.left : parent.right
        anchors.rightMargin: Metrics.gap12
        spacing: Metrics.gap2

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Metrics.gap6

            Text {
                width: Math.min(implicitWidth, Math.max(0, labels.width - (indicator.visible ? indicator.width + Metrics.gap6 : 0))) // derived: reserve the quality icon inside the text area
                text: Player.title
                elide: Text.ElideRight
                font.pointSize: Type.row
                font.weight: Type.medium
                color: Theme.color.textPrimary
            }

            QualityIcon {
                id: indicator
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
            readonly property real textWidth: Math.max(0, width - (origin.visible ? origin.width + Metrics.gap6 : 0))

            Row {
                anchors.centerIn: parent
                height: parent.height
                spacing: Metrics.gap6

                IconButton {
                    id: origin
                    objectName: "playerOrigin"
                    visible: Player.origin !== ""
                    size: Metrics.button26
                    anchors.verticalCenter: parent.verticalCenter
                    glyph: Icons.stack2
                    tip: I18n.tr("Go to source", I18n.rev)
                    onClicked: Navigation.goOrigin(Player.origin)
                }

                CreditLine {
                    id: artist
                    anchors.verticalCenter: parent.verticalCenter
                    objectName: "playerArtist"
                    width: Player.album === "" ? Math.min(implicitWidth, context.textWidth) : Math.min(implicitWidth, (context.textWidth - separator.implicitWidth - Metrics.gap12) / 2) // derived: share the available context row
                    credit: Player.artist; trackPath: Player.path
                    entry: lcd.onAir
                    parentSource: lcd.parentSource
                    font.pointSize: Type.small
                    color: hovered ? Theme.color.textPrimary : Theme.color.textSecondary
                }

                Text {
                    id: separator
                    anchors.verticalCenter: parent.verticalCenter
                    visible: Player.artist !== "" && Player.album !== ""
                    text: "—"
                    font.pointSize: Type.small
                    color: Theme.color.textSecondary
                }

                Text {
                    id: album
                    anchors.verticalCenter: parent.verticalCenter
                    objectName: "playerAlbum"
                    visible: Player.album !== ""
                    width: Player.artist === "" ? Math.min(implicitWidth, context.textWidth) : Math.min(implicitWidth, (context.textWidth - separator.implicitWidth - Metrics.gap12) / 2) // derived: share the available context row
                    text: Player.album
                    elide: Text.ElideRight
                    font.pointSize: Type.small
                    color: albumHover.hovered && (lcd.parentSource !== "" || Player.albumId !== 0) ? Theme.color.textPrimary : Theme.color.textSecondary

                    HoverHandler {
                        id: albumHover
                        enabled: (lcd.parentSource !== "" || Player.albumId !== 0)
                        cursorShape: Qt.PointingHandCursor
                    }
                    TapHandler {
                        enabled: (lcd.parentSource !== "" || Player.albumId !== 0)
                        onTapped: {
                            if (lcd.parentSource !== "") { Sources.command("playing_parent", "", ""); Navigation.goKey("source", lcd.parentSource) }
                            else Navigation.go("album", Player.albumId)
                        }
                    }
                }
            }
        }
    }

    SeekBar {
        objectName: "playerSeek"
        visible: !Player.live && !lcd.tall
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: lcd.radius
        anchors.rightMargin: lcd.radius
        position: Player.position
        duration: Player.duration
        bubbleText: Player.clock(hoverSeconds)
        loopStart: Controls.loopState > 0 ? Controls.loopStart : -1
        loopEnd: Controls.loopState > 1 ? Controls.loopEnd : -1
        onSeek: (seconds) => Player.seek(seconds)
    }
}
