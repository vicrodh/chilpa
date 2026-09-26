import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Components"

// Settings (design/06 Settings): the sections on the left, the chosen one's
// rows on the right. Every row comes from the schema in Rust.
Item {
    id: page

    readonly property string section: Navigation.section !== "" ? Navigation.section : SettingsModel.sections[0]

    SettingsPage {
        id: rows
        section: page.section
    }

    Text {
        id: title
        x: Metrics.contentX
        y: Metrics.contentTop
        text: I18n.tr("Settings", I18n.rev)
        font.pointSize: Type.title
        font.weight: Type.bold
        font.letterSpacing: Type.titleTracking
        color: Theme.color.textPrimary
    }

    Column {
        id: nav
        x: Metrics.contentX
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section
        width: Metrics.settingsNav

        Repeater {
            model: SettingsModel.sections

            NavItem {
                required property int index
                required property string modelData
                width: nav.width
                text: SettingsModel.sectionLabels[index]
                glyph: Icons[SettingsModel.sectionIcons[index]] || ""
                active: modelData === page.section
                onClicked: Navigation.openSection(modelData)
            }
        }
    }

    ListView {
        anchors.top: title.bottom
        anchors.topMargin: Metrics.section - Metrics.settingsGroup + Metrics.gap12
        anchors.bottom: parent.bottom
        anchors.left: nav.right
        anchors.leftMargin: Metrics.heroGap
        // To the right edge, whatever the window's width: every row's control
        // sits on the right, its label and description on the left.
        width: page.width - x - Metrics.contentX
        clip: true
        model: rows
        bottomMargin: Metrics.contentBottom
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        delegate: SettingsRow {
            width: ListView.view.width - Metrics.gap12
            page: rows
        }
    }

    Text {
        visible: rows.pending
        anchors.left: nav.right
        anchors.leftMargin: Metrics.heroGap
        anchors.top: nav.top
        text: I18n.tr("This section is not built yet.", I18n.rev)
        font.pointSize: Type.body
        color: Theme.color.textSecondary
    }
}
