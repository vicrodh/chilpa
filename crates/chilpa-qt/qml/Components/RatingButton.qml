import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The stars of what a page is about, behind one round button like the
// others beside it: it opens a flyout, and the rating is made there. The
// flyout takes every click on it, a click outside closes it and is nothing
// else, and once a rating is made it shows it for a moment and goes.
Item {
    id: control

    // "album", "playlist" or "track", and which one.
    property string kind
    property double target: 0
    property bool contrasted: false

    readonly property int value: target > 0 ? Ratings.stars(kind, target, Ratings.rev) : 0

    implicitWidth: opener.implicitWidth
    implicitHeight: opener.implicitHeight
    visible: target > 0

    // What it is about changed under it: the flyout was for the other one.
    onTargetChanged: flyout.close()

    IconButton {
        id: opener
        round: true
        glyph: control.value > 0 ? Icons.starFilled : Icons.star
        filled: control.value > 0
        checked: flyout.opened || control.value > 0
        tone: control.contrasted ? "overlay" : control.value > 0 ? "accent" : "secondary"
        tip: control.value > 0 ? I18n.trCount("{} star", "{} stars", control.value, I18n.rev)
                               : I18n.tr("Rate", I18n.rev)
        onClicked: flyout.opened ? flyout.close() : flyout.open()
    }

    Popup {
        id: flyout

        modal: true
        dim: false
        x: (control.width - width) / 2 // derived: centered under its button
        y: control.height + Metrics.gap6
        padding: Metrics.gap12
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        onClosed: settle.stop()

        background: Item {
            Hold {}
            RectangularShadow {
                anchors.fill: panel
                radius: panel.radius
                offset.y: Metrics.shadowElevatedY
                blur: Metrics.shadowElevatedBlur
                color: Theme.color.shadowElevated
            }
            Rectangle {
                id: panel
                anchors.fill: parent
                radius: Metrics.radiusMenu
                color: Theme.color.surfaceRaised
                Edge {}
            }
        }

        contentItem: Column {
            spacing: Metrics.gap8

            RatingStars {
                anchors.horizontalCenter: parent.horizontalCenter
                size: Metrics.ratingStarLarge
                value: control.value
                onRated: (stars) => {
                    Ratings.rate(control.kind, control.target, stars)
                    settle.restart()
                }
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: control.value > 0 ? I18n.trCount("{} star", "{} stars", control.value, I18n.rev)
                                        : I18n.tr("Not rated", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
        }
    }

    Timer {
        id: settle
        interval: Motion.ratingSettle
        onTriggered: flyout.close()
    }
}
