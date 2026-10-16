import QtQuick
import QtQuick.Effects
import app.chilpa
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Primitives"
import "../Theme"

// The small card at the bottom right over Wave Bed, the one-line lyrics,
// the scopes and the scenes (03-immersive §1.1): title 15/600 capped at
// 420, album italic and artist never cut, the quality chip, and the cover
// at 72 with its shadow. It hugs its content and grows leftward; it never
// fades with the chrome.
Item {
    id: card
    property bool shaders: true
    readonly property real metaWide: Math.max(title.implicitWidth, sub.implicitWidth, chip.width)
    readonly property real metaTall: title.height + Stage.queueHistoryApart + sub.height + Stage.queueHistoryApart + chip.height
    implicitWidth: metaWide + Stage.menuRowPad + Stage.cardCover
    implicitHeight: Math.max(metaTall, Stage.cardCover)
    width: implicitWidth
    height: implicitHeight

    Text {
        id: title
        x: card.metaWide - width
        width: Math.min(implicitWidth, Stage.cardTitleCap)
        text: Player.title
        color: Theme.color.immersiveTextStrong
        font.pointSize: Stage.cardTitleText
        font.weight: Type.semibold
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignRight
    }
    Row {
        id: sub
        x: card.metaWide - width
        y: title.height + Stage.queueHistoryApart
        spacing: Stage.queueHistoryApart
        Text { visible: Player.album !== ""; text: Player.album; color: Theme.color.immersiveTextGhost; font.pointSize: Stage.cardSubText; font.italic: true }
        Text { visible: Player.album !== "" && Player.artist !== ""; text: "·"; color: Theme.color.immersiveTextQuiet; font.pointSize: Stage.cardSubText }
        Text { visible: Player.artist !== ""; text: Player.artist; color: Theme.color.immersiveTextFaint; font.pointSize: Stage.cardSubText }
    }
    QualityChip {
        id: chip
        x: card.metaWide - width
        y: title.height + Stage.queueHistoryApart + sub.height + Stage.queueHistoryApart
        text: Player.qualityLong
    }
    Rectangle {
        x: card.width - Stage.cardCover
        y: Stage.cardShadowDrop
        width: Stage.cardCover
        height: Stage.cardCover
        radius: Stage.cardCoverRound
        color: Theme.color.immersiveShadowArt
        layer.enabled: card.shaders
        layer.effect: MultiEffect { blurEnabled: true; blurMax: Stage.cardShadow; blur: 1 }
    }
    ArtTile {
        x: card.width - Stage.cardCover
        width: Stage.cardCover
        height: Stage.cardCover
        radius: Stage.cardCoverRound
        edged: false
        source: Player.art
    }
}
