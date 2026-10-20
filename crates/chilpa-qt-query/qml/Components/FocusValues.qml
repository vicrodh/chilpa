import QtQuick
import QtQuick.Controls.Basic as Controls
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The open category owns one facet model and only viewport delegates.
Item {
    id: picker
    property string field
    signal picked(string value, bool excluded)
    FacetModel { id: facets; column: picker.field; rev: Library.rev; query: search.text }
    TextField {
        id: search
        objectName: "focusSearch"
        width: parent.width
        placeholder: I18n.tr("Search", I18n.rev)
    }
    ListView {
        objectName: "focusValues"
        anchors.top: search.bottom
        anchors.topMargin: Metrics.gap4
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        reuseItems: true
        model: facets
        Controls.ScrollBar.vertical: ViewportScrollBar {}
        delegate: Item {
            id: entry
            required property string key
            required property string label
            required property string meta
            required property bool all
            width: ListView.view.width
            height: all ? 0 : Metrics.paneRow
            visible: !all
            Rectangle { anchors.fill: parent; radius: Metrics.radiusRow; color: hover.hovered ? Theme.color.stateHover : "transparent" }
            Text {
                anchors.left: parent.left
                anchors.right: count.left
                anchors.verticalCenter: parent.verticalCenter
                text: entry.label
                elide: Text.ElideRight
                font.pointSize: Type.row
                color: Theme.color.textPrimary
            }
            Text {
                id: count
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: picker.field === "album" && entry.meta ? I18n.tr("Year", I18n.rev) + ": " + entry.meta : entry.meta
                font.pointSize: Type.meta
                color: Theme.color.textSecondary
            }
            HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
            TapHandler {
                id: tap
                onTapped: picker.picked(picker.field === "album" ? entry.label : entry.key, !!(tap.point.modifiers & Qt.AltModifier))
            }
        }
    }
}
