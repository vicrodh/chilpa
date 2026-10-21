import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// I keep the scope control beside the query chips.
Item {
    id: crumbs

    readonly property bool filtered: Library.extraFiltered || Library.queryText !== ""

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

    }
}
