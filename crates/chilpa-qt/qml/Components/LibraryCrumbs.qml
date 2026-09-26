import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// The Library breadcrumb: "All music", then one crumb per chosen column,
// each with an x that clears it, and "Clear filters" on the right.
Item {
    id: crumbs

    readonly property bool filtered: Library.year !== "" || Library.genre !== "" || Library.artist !== "" || Library.album !== "" || Library.extraFiltered

    Row {
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
                TapHandler { onTapped: Library.clear() }
            }
        }

        Repeater {
            model: [
                { column: "year", name: I18n.tr("Year", I18n.rev), value: Library.year },
                { column: "genre", name: I18n.tr("Genre", I18n.rev), value: Library.genre },
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
