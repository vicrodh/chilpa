import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import app.chilpa.about
import app.chilpa.wizard
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// QBZ's What's New deck (qbz-qt/qml/shell/WhatsNewModal.qml), for the Welcome
// on a new install and for What's new after an update. One accent button
// walks the cards; after What's new's last card it opens the release notes,
// and from there, or after the Welcome's last card, it closes. The dots and
// the arrows move too. Nothing animates and nothing advances by itself, as
// in QBZ. Escape, the X, a click outside and that last button close it and
// mark the version seen; a card's button closes it and opens the HiFi Wizard
// or asks the window to go where the card says. It asks the facade what this
// start shows until the plugins are up; the menu opens What's new any time.
Modal {
    id: modal

    signal actionChosen(string action)

    readonly property var doc: {
        try {
            return JSON.parse(Onboarding.doc)
        } catch (e) {
            return ({})
        }
    }
    readonly property bool welcome: doc.kind === "welcome"
    readonly property var deck: doc.deck || []
    property int cardIndex: 0
    readonly property bool showingNotes: !welcome && cardIndex >= deck.length
    readonly property int cardCount: deck.length + (welcome ? 0 : 1)
    readonly property var card: showingNotes ? null : deck[cardIndex] || null
    readonly property bool lastCard: cardIndex === cardCount - 1

    width: Math.min((parent ? parent.width : OnboardingFrame.widest) - OnboardingFrame.gutter, OnboardingFrame.widest)
    height: Math.min((parent ? parent.height : OnboardingFrame.tallest) - OnboardingFrame.gutter, OnboardingFrame.tallest)
    Overlay.modal: Rectangle { color: Theme.color.lightboxScrim }

    readonly property bool wanted: doc.open === true
    onWantedChanged: {
        if (!wanted)
            return close()
        cardIndex = 0
        open()
    }
    onOpened: keys.forceActiveFocus()
    onClosed: if (wanted) Onboarding.command("close")

    function goBack() { if (cardIndex > 0) cardIndex -= 1 }
    function goNext() { if (lastCard) close(); else cardIndex += 1 }
    // The HiFi Wizard opens here, as its Settings button opens it; anything
    // else is a place in the window, for the window to go to.
    function follow(action) {
        close()
        if (action === "hifiWizard")
            HifiWizard.command("open", "")
        else
            actionChosen(action)
    }

    // Asks what this start shows until the facade has an answer, then reads
    // the notes while they load.
    Timer { interval: OnboardingFrame.startMs; repeat: true; running: modal.doc.started !== true; onTriggered: Onboarding.command("start") }
    Timer { interval: OnboardingFrame.readMs; repeat: true; running: modal.visible && modal.doc.loading === true; onTriggered: Onboarding.refresh() }

    Item {
        id: keys
        width: modal.width - Metrics.modalInsets
        height: modal.height - Metrics.modalInsets
        focus: true
        Keys.onLeftPressed: modal.goBack()
        Keys.onRightPressed: modal.goNext()

        Item {
            anchors.fill: parent
            anchors.margins: OnboardingFrame.inset

            Item {
                id: header
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: OnboardingFrame.header

                Text {
                    objectName: "deckHeading"
                    anchors.left: parent.left
                    anchors.right: shut.left
                    anchors.rightMargin: Metrics.gap10
                    anchors.verticalCenter: parent.verticalCenter
                    text: {
                        if (modal.welcome)
                            return I18n.tr("Welcome to Chilpa", I18n.rev)
                        const version = modal.doc.version || ""
                        if (version === "")
                            return I18n.tr("What's new", I18n.rev)
                        // The release's civil date, in the reader's locale.
                        let date = modal.doc.date || ""
                        const day = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date)
                        if (day)
                            date = Qt.formatDate(new Date(Number(day[1]), Number(day[2]) - 1, Number(day[3])), Locale.LongFormat)
                        return I18n.tr("What's new in v{}", I18n.rev).replace("{}", version) + (date === "" ? "" : " (" + date + ")")
                    }
                    color: Theme.color.textPrimary
                    font.pointSize: OnboardingFrame.headingSize
                    font.weight: Type.semibold
                    elide: Text.ElideRight
                }
                IconButton {
                    id: shut
                    objectName: "closeDeck"
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    size: OnboardingFrame.closeBox
                    glyph: Icons.x
                    tip: I18n.tr("Close", I18n.rev)
                    onClicked: modal.close()
                }
            }

            Item {
                anchors.top: header.bottom
                anchors.topMargin: OnboardingFrame.gap
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: footer.top
                anchors.bottomMargin: OnboardingFrame.gap

                DeckCard {
                    objectName: "deckCard"
                    anchors.fill: parent
                    visible: !modal.showingNotes
                    // Closed, the deck holds no card and so no decoded art.
                    model: modal.visible ? modal.card : null
                    eyebrow: modal.welcome ? I18n.tr("Getting started", I18n.rev)
                                           : I18n.tr("New in {}", I18n.rev).replace("{}", modal.doc.version || "")
                    onActionChosen: action => modal.follow(action)
                }
                ReleaseNotes {
                    objectName: "releaseNotes"
                    anchors.fill: parent
                    visible: modal.showingNotes
                    doc: modal.doc
                }
            }

            Item {
                id: footer
                anchors.bottom: parent.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                height: OnboardingFrame.footer

                Row {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: OnboardingFrame.dotGap
                    visible: modal.cardCount > 1
                    Repeater {
                        objectName: "deckDots"
                        model: modal.cardCount
                        Rectangle {
                            required property int index
                            width: OnboardingFrame.dot
                            height: OnboardingFrame.dot
                            radius: width / 2 // derived: a circle
                            color: index === modal.cardIndex ? Theme.color.accent : "transparent"
                            border.width: 1
                            border.color: index === modal.cardIndex ? Theme.color.accent : Theme.color.strokeHairline
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: modal.cardIndex = index
                            }
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Metrics.gap8
                    PillButton {
                        objectName: "deckBack"
                        visible: modal.cardIndex > 0
                        height: OnboardingFrame.footer
                        text: I18n.tr("Back", I18n.rev)
                        onClicked: modal.goBack()
                    }
                    PillButton {
                        objectName: "deckNext"
                        kind: "primary"
                        height: OnboardingFrame.footer
                        text: modal.showingNotes ? I18n.tr("Close", I18n.rev)
                            : !modal.lastCard ? (modal.welcome || modal.cardIndex < modal.deck.length - 1 ? I18n.tr("Next", I18n.rev) : I18n.tr("Release notes", I18n.rev))
                            : I18n.tr("Get started", I18n.rev)
                        onClicked: modal.goNext()
                    }
                }
            }
        }
    }
}
