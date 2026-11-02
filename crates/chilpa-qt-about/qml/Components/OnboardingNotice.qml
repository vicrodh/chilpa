import QtQuick
import app.chilpa.about
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

Rectangle {
    id: notice
    objectName: "onboardingNotice"

    readonly property var doc: {
        try {
            return JSON.parse(Onboarding.doc)
        } catch (e) {
            return ({})
        }
    }
    readonly property bool shown: doc.aside === true
    readonly property int cards: doc.welcomeCards || 0
    readonly property real progress: cards > 0 ? Math.min((doc.reached || 0) / cards, 1) : 0
    readonly property color track: Theme.color.strokeHairline
    readonly property color fill: Theme.color.accent

    onProgressChanged: ring.requestPaint()
    onTrackChanged: ring.requestPaint()
    onFillChanged: ring.requestPaint()

    implicitHeight: Metrics.navItemHeight
    radius: Metrics.radiusControl
    color: area.containsMouse ? Theme.color.stateHover : Theme.color.fillField

    MouseArea {
        id: area
        objectName: "continueOnboarding"
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        Accessible.role: Accessible.Button
        Accessible.name: label.text
        onClicked: Onboarding.command("welcome")
    }
    Canvas {
        id: ring
        objectName: "onboardingRing"
        anchors.left: parent.left
        anchors.leftMargin: Metrics.navItemPadding
        anchors.verticalCenter: parent.verticalCenter
        width: Metrics.iconNav
        height: Metrics.iconNav
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            const c = width / 2, r = c - OnboardingFrame.ringStroke / 2 // derived: the stroke sits inside the box
            ctx.lineWidth = OnboardingFrame.ringStroke
            ctx.strokeStyle = notice.track
            ctx.beginPath()
            ctx.arc(c, c, r, 0, 2 * Math.PI)
            ctx.stroke()
            if (notice.progress <= 0)
                return
            ctx.strokeStyle = notice.fill
            ctx.beginPath()
            ctx.arc(c, c, r, -Math.PI / 2, -Math.PI / 2 + 2 * Math.PI * notice.progress)
            ctx.stroke()
        }
    }
    Text {
        id: label
        anchors.left: ring.right
        anchors.leftMargin: Metrics.gap10
        anchors.right: dismiss.left
        anchors.verticalCenter: parent.verticalCenter
        text: I18n.tr("Onboarding", I18n.rev)
        elide: Text.ElideRight
        font.pointSize: Type.body
        color: Theme.color.textPrimary
    }
    IconButton {
        id: dismiss
        objectName: "dismissOnboarding"
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        glyph: Icons.x
        tip: I18n.tr("Dismiss", I18n.rev)
        onClicked: Onboarding.command("dismiss")
    }
}
