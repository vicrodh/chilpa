import QtQuick
import com.blitzfc.chilpa
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/com/blitzfc/chilpa/kit/qml/Theme"

// A cover the metadata editor found: its preview, kept on disk by the
// editor, and what it belongs to. A click chooses it. `candidate` is one
// entry of the editor's view.
Item {
    id: root

    property var candidate: ({})

    signal choose()

    implicitWidth: Metrics.metadataCandidate
    implicitHeight: art.height + Metrics.gap4 + caption.height

    ArtTile {
        id: art
        width: parent.width
        height: width
        path: root.candidate.preview || ""
        opacity: pointer.hovered ? Metrics.disabledOpacity * 2 : 1 // derived: dimmed under the pointer, never hidden
    }

    Text {
        id: caption
        y: art.height + Metrics.gap4
        width: parent.width
        text: root.candidate.detail || root.candidate.title || ""
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignHCenter
        font.pointSize: Type.small
        color: Theme.color.textSecondary
    }

    HoverHandler {
        id: pointer
        cursorShape: Qt.PointingHandCursor
        onHoveredChanged: hovered ? TipState.show(root.candidate.title || "", art) : TipState.hide(art)
    }
    TapHandler { onTapped: root.choose() }
}
