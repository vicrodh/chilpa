import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// The player's "LCD": cover, times, title with its quality, who and from
// what, and the progress along its lower edge. A click on the edge seeks.
Rectangle {
    id: lcd

    property bool showTimes: true

    height: Metrics.lcdHeight
    radius: Metrics.radiusLcd
    color: Theme.color.surfaceLcd

    ArtTile {
        id: cover
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
        visible: lcd.showTimes && Player.playing
        text: Player.remaining
        font.pointSize: Type.micro
        font.features: { "tnum": 1 }
        color: Theme.color.textTertiary
    }

    Column {
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
                anchors.verticalCenter: parent.verticalCenter
                tier: Player.qualityTier

                HoverHandler { id: quality }
            }
        }

        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: Player.album === "" ? Player.artist : Player.artist + " — " + Player.album
            elide: Text.ElideRight
            font.pointSize: Type.small
            color: Theme.color.textSecondary
        }
    }

    QualityTooltip {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.top
        anchors.bottomMargin: Metrics.gap22
        visible: quality.hovered && Player.playing
    }

    Rectangle {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: lcd.radius
        anchors.rightMargin: lcd.radius
        height: Metrics.progress
        color: Theme.color.strokeHairline

        Rectangle {
            width: Player.duration > 0 ? parent.width * Math.min(1, Player.position / Player.duration) : 0
            height: parent.height
            color: Theme.color.textSecondary
        }

        MouseArea {
            anchors.fill: parent
            anchors.topMargin: -Metrics.gap6
            anchors.bottomMargin: -Metrics.gap4
            enabled: Player.duration > 0
            onClicked: (mouse) => Player.seek(Player.duration * mouse.x / width)
        }
    }
}
