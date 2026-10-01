import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The line under a source's title. Down its tree, the way back: the source,
// then each step, the last one where the window stands. At a root narrowed
// by columns, what is picked in them, each with an x that clears it. Chilpa's
// own folders of that source sit at its right end.
Item {
    id: line

    // The snapshot the source screen reads.
    property var view: ({})

    signal stepped(int index)
    signal cleared(string column)
    signal opened(string id)

    readonly property var crumbs: view.crumbs || []
    readonly property var picked: (view.columns || []).filter(c => c.picked !== "")

    Row {
        objectName: "sourceCrumbs"
        anchors.verticalCenter: parent.verticalCenter
        visible: line.crumbs.length > 0
        spacing: Metrics.gap6

        Repeater {
            model: [line.view.name || ""].concat(line.crumbs)

            Text {
                required property string modelData
                required property int index
                anchors.verticalCenter: parent.verticalCenter
                text: index > 0 ? "›  " + modelData : modelData
                font.pointSize: Type.body
                font.weight: index === line.crumbs.length ? Type.medium : Type.regular
                color: index === line.crumbs.length ? Theme.color.textPrimary : Theme.color.textSecondary

                HoverHandler { cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: line.stepped(index) }
            }
        }
    }

    Row {
        anchors.verticalCenter: parent.verticalCenter
        visible: line.view.explorer === true
        spacing: Metrics.gap8

        Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.filter; size: Metrics.iconSm }
        Repeater {
            model: line.picked

            FilterCrumb {
                required property var modelData
                objectName: "sourcePick"
                anchors.verticalCenter: parent.verticalCenter
                name: modelData.title
                value: modelData.pickedTitle
                onRemoved: line.cleared(modelData.id)
            }
        }
    }

    Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Repeater {
            model: line.view.shortcuts || []

            PillButton {
                required property var modelData
                kind: "ghost"
                // The play glyph exists only in the filled set.
                glyph: modelData.id === "chilpa:saved" ? Icons.heart : Icons.playerPlayFilled
                filled: modelData.id !== "chilpa:saved"
                text: modelData.title
                onClicked: line.opened(modelData.id)
            }
        }
    }
}
