import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// Where a folder is: "Folders", then each folder on the way down to it.
// Each step opens that folder.
Flow {
    id: crumbs

    property string path

    readonly property var steps: {
        const parts = path.split("/").filter((p) => p !== "")
        const out = []
        for (let i = 0; i < parts.length; ++i)
            out.push({ name: parts[i], path: "/" + parts.slice(0, i + 1).join("/") })
        return out
    }

    signal opened(string path)

    spacing: Metrics.gap4

    Text {
        text: I18n.tr("Folders", I18n.rev)
        font.pointSize: Type.meta
        color: Theme.color.accent
        TapHandler { onTapped: crumbs.opened("") }
    }

    Repeater {
        model: crumbs.steps

        Row {
            required property var modelData
            required property int index
            spacing: Metrics.gap4

            Icon { glyph: Icons.chevronRight; size: Metrics.iconXs; color: Theme.color.textTertiary; anchors.verticalCenter: parent.verticalCenter }
            Text {
                text: modelData.name
                font.pointSize: Type.meta
                color: index === crumbs.steps.length - 1 ? Theme.color.textPrimary : Theme.color.textSecondary
                TapHandler { onTapped: crumbs.opened(modelData.path) }
            }
        }
    }
}
