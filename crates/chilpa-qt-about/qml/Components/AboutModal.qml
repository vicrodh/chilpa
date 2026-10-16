import QtQuick
import QtQuick.Controls.Basic
import app.chilpa.kit
import app.chilpa.about
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Theme"

// About Chilpa, laid out as QBZ's AboutModal: the logo, the name and the
// version over a rule, and under it, scrolling, what Chilpa is, where to
// read more, which build this is and whom it thanks. Escape, the close
// button or a click outside closes it.
Modal {
    id: about
    readonly property var doc: {
        try {
            return JSON.parse(About.document(I18n.rev))
        } catch (e) {
            return ({})
        }
    }
    width: Math.min((parent ? parent.width : Frame.widest) - Frame.gutter, Frame.widest)
    height: Math.min((parent ? parent.height : Frame.tallest) - Frame.gutter, Frame.tallest)

    Item {
        width: about.width - Metrics.modalInsets
        height: about.height - Metrics.modalInsets

        Item {
            id: head
            width: parent.width
            height: Frame.head
            Image {
                id: logo
                width: Frame.head
                height: Frame.head
                source: "qrc:/qt/qml/app/chilpa/kit/assets/logo.png"
                sourceSize: Qt.size(Frame.logoDecode, Frame.logoDecode)
                fillMode: Image.PreserveAspectFit
                smooth: true
                mipmap: true
            }
            Column {
                anchors.left: logo.right
                anchors.leftMargin: Metrics.gap16
                anchors.right: close.left
                anchors.rightMargin: Metrics.gap16
                anchors.verticalCenter: parent.verticalCenter
                spacing: Metrics.gap2
                Text {
                    objectName: "aboutName"
                    text: about.doc.name || ""
                    font.pointSize: Type.titleSmall
                    font.weight: Type.bold
                    color: Theme.color.textPrimary
                }
                Text {
                    objectName: "aboutVersion"
                    text: "v" + (about.doc.version || "")
                    font.pointSize: Type.small
                    color: Theme.color.textTertiary
                }
            }
            IconButton {
                id: close
                objectName: "aboutClose"
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                glyph: Icons.x
                tip: I18n.tr("Close", I18n.rev)
                onClicked: about.close()
            }
        }
        Rectangle {
            id: rule
            anchors.top: head.bottom
            anchors.topMargin: Metrics.gap16
            width: parent.width
            height: 1
            color: Theme.color.strokeHairline
        }
        Flickable {
            id: flick
            anchors.top: rule.bottom
            anchors.topMargin: Metrics.gap16
            anchors.bottom: parent.bottom
            width: parent.width
            clip: true
            contentWidth: width
            contentHeight: body.height
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar {}
            AboutBody {
                id: body
                width: flick.width
                doc: about.doc
            }
        }
    }
}
