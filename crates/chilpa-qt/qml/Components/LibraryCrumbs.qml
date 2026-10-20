import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// The Library breadcrumb: "All music", then one crumb per chosen column,
// each with an x that clears it. The screen sets the conditions stacked
// beyond the columns right after `used`.
Item {
    id: crumbs

    readonly property bool filtered: Library.year !== "" || Library.genre !== "" || Library.label !== "" || Library.composer !== "" || Library.rating !== "" || Library.artist !== "" || Library.album !== "" || Library.extraFiltered || Library.queryText !== ""

    // How much of the width the crumbs take.
    readonly property real used: line.width

    Row {
        id: line
        anchors.verticalCenter: parent.verticalCenter
        spacing: Metrics.gap8

        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap6
            Icon { anchors.verticalCenter: parent.verticalCenter; glyph: Icons.filter; size: Metrics.iconSm }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: I18n.tr("All music", I18n.rev)
                font.pointSize: Type.body
                font.weight: crumbs.filtered ? Type.regular : Type.medium
                color: crumbs.filtered ? Theme.color.textSecondary : Theme.color.textPrimary
                HoverHandler { cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: Library.clear() }
            }
        }

        Repeater {
            model: [
                { column: "year", name: I18n.tr("Year", I18n.rev), value: Library.year },
                { column: "genre", name: I18n.tr("Genre", I18n.rev), value: Library.genre },
                { column: "label", name: I18n.tr("Label", I18n.rev), value: Library.label },
                { column: "composer", name: I18n.tr("Composer", I18n.rev), value: Library.composer },
                { column: "rating", name: I18n.tr("Rating", I18n.rev), value: Library.rating },
                { column: "artist", name: I18n.tr("Artist", I18n.rev), value: Library.artist },
                { column: "album", name: I18n.tr("Album", I18n.rev), value: Library.album }
            ]

            FilterCrumb {
                required property var modelData
                anchors.verticalCenter: parent.verticalCenter
                visible: modelData.value !== ""
                name: modelData.name
                value: modelData.value
                onRemoved: Library.choose(modelData.column, "", "")
            }
        }
    }

}
