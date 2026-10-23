import QtQuick
import QtQuick.Window
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// One card of the deck, QBZ's (WhatsNewModal.qml): the art fills the box,
// darkened from the left by a gradient painted here, and the text sits over
// the left half: an eyebrow, the title, a sentence or two and, when the card
// leads somewhere, a button.
Rectangle {
    id: card

    property var model: null
    property string eyebrow
    signal actionChosen(string action)

    radius: Metrics.radiusControl
    color: Theme.color.surfaceRaised
    clip: true

    Image {
        anchors.fill: parent
        source: card.model ? OnboardingFrame.art + card.model.image : ""
        // Decoded at the painted size, not at the file's, and not kept in
        // the image cache: a card is seen once, and its pixels go with it.
        sourceSize.width: Math.round(width * Screen.devicePixelRatio)
        sourceSize.height: Math.round(height * Screen.devicePixelRatio)
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: false
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Theme.color.onboardingArtFrom }
            GradientStop { position: OnboardingFrame.artMiddleAt; color: Theme.color.onboardingArtMiddle }
            GradientStop { position: 1; color: Theme.color.onboardingArtTo }
        }
    }

    Column {
        anchors.left: parent.left
        anchors.leftMargin: OnboardingFrame.cardInset
        anchors.right: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        spacing: OnboardingFrame.cardGap

        Text {
            text: card.eyebrow
            color: Theme.color.accent
            font.pointSize: OnboardingFrame.eyebrowSize
            font.weight: Type.bold
            font.letterSpacing: OnboardingFrame.eyebrowTracking
        }
        Text {
            objectName: "cardTitle"
            width: parent.width
            text: card.model ? card.model.title : ""
            color: Theme.color.backdropText
            font.pointSize: OnboardingFrame.titleSize
            font.weight: Type.bold
            wrapMode: Text.WordWrap
        }
        Text {
            width: parent.width
            text: card.model ? card.model.body : ""
            color: Theme.color.backdropTextMuted
            font.pointSize: OnboardingFrame.bodySize
            lineHeight: OnboardingFrame.bodyLine
            wrapMode: Text.WordWrap
        }
        Item { width: 1; height: Metrics.gap4; visible: action.visible }
        PillButton {
            id: action
            objectName: "cardAction"
            visible: !!card.model && (card.model.action || "") !== ""
            kind: "primary"
            height: OnboardingFrame.cardButton
            text: card.model ? card.model.actionLabel || "" : ""
            onClicked: card.actionChosen(card.model.action)
        }
    }
}
