import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.stats
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Patterns"

Modal {
    id: dialog
    objectName: "analysisInventoryDialog"
    property var inventorySource: Stats
    property string filter: "all"
    property var summary: ({ total: 0, standard: 0, mood: 0, matched: 0 })
    property var tracks: []
    property string error: ""
    width: Math.min(Metrics.analysisInventoryWidth, parent.width - Metrics.modalInset)
    height: Math.min(Metrics.analysisInventoryHeight, parent.height - Metrics.modalInset)

    function show() {
        filter = "all"
        search.text = ""
        open()
        fetch(true)
    }
    function fetch(reset) {
        if (!inventorySource) return
        try {
            const start = reset ? 0 : tracks.length
            const answer = JSON.parse(inventorySource.inventory(filter, search.text, start))
            if (answer.error) {
                error = answer.error
                return
            }
            error = ""
            summary = answer
            tracks = reset ? answer.tracks : tracks.concat(answer.tracks)
            if (reset) list.positionViewAtBeginning()
        } catch (e) {
            error = I18n.tr("Could not read the analysis inventory.", I18n.rev)
        }
    }
    function choose(value) {
        if (filter === value) return
        filter = value
        fetch(true)
    }

    Timer {
        id: searchDelay
        interval: 250
        onTriggered: dialog.fetch(true)
    }

    Item {
        anchors.fill: parent
        Row {
            id: heading
            width: parent.width
            Text {
                width: parent.width - closeButton.width
                text: I18n.tr("Analysis inventory", I18n.rev)
                font.pointSize: Type.titleSmall
                font.weight: Type.bold
                color: Theme.color.textPrimary
            }
            IconButton { id: closeButton; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: dialog.close() }
        }
        Text {
            id: counts
            anchors.top: heading.bottom
            anchors.topMargin: Metrics.gap12
            width: parent.width
            text: I18n.tr("Local tracks", I18n.rev) + ": " + summary.total
                + "  ·  " + I18n.tr("Standard analyzed", I18n.rev) + ": " + summary.standard
                + "  ·  " + I18n.tr("Mood stored", I18n.rev) + ": " + summary.mood
            font.pointSize: Type.body
            color: Theme.color.textSecondary
        }
        Kit.SearchField {
            id: search
            anchors.top: counts.bottom
            anchors.topMargin: Metrics.gap12
            width: parent.width
            placeholder: I18n.tr("Search title, artist or album", I18n.rev)
            onTextChanged: searchDelay.restart()
            onAccepted: { searchDelay.stop(); dialog.fetch(true) }
        }
        Row {
            id: filters
            anchors.top: search.bottom
            anchors.topMargin: Metrics.gap12
            spacing: Metrics.gap6
            PillButton { text: I18n.tr("All", I18n.rev); kind: dialog.filter === "all" ? "primary" : "secondary"; onClicked: dialog.choose("all") }
            PillButton { text: I18n.tr("Standard done", I18n.rev); kind: dialog.filter === "standard" ? "primary" : "secondary"; onClicked: dialog.choose("standard") }
            PillButton { text: I18n.tr("Mood done", I18n.rev); kind: dialog.filter === "mood" ? "primary" : "secondary"; onClicked: dialog.choose("mood") }
            PillButton { text: I18n.tr("Standard pending", I18n.rev); kind: dialog.filter === "standard-pending" ? "primary" : "secondary"; onClicked: dialog.choose("standard-pending") }
            PillButton { text: I18n.tr("Mood pending", I18n.rev); kind: dialog.filter === "mood-pending" ? "primary" : "secondary"; onClicked: dialog.choose("mood-pending") }
        }
        Text {
            id: notice
            anchors.top: filters.bottom
            anchors.topMargin: Metrics.gap12
            width: parent.width
            text: dialog.error !== "" ? dialog.error : I18n.tr("Showing", I18n.rev) + " " + dialog.tracks.length + " / " + dialog.summary.matched
            font.pointSize: Type.meta
            color: dialog.error !== "" ? Theme.color.statusWarning : Theme.color.textTertiary
        }
        ListView {
            id: list
            anchors.top: notice.bottom
            anchors.topMargin: Metrics.gap8
            anchors.bottom: footer.top
            anchors.bottomMargin: Metrics.gap12
            width: parent.width
            model: dialog.tracks
            clip: true
            spacing: Metrics.gap2
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ViewportScrollBar {}
            delegate: Rectangle {
                required property var modelData
                required property int index
                width: ListView.view.width - Metrics.gap12
                height: Metrics.settingsRow
                radius: Metrics.radiusControl
                color: index % 2 === 0 ? Theme.color.stateHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: Metrics.gap12
                    anchors.rightMargin: Metrics.gap12
                    spacing: Metrics.gap12
                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width - standardLabel.width - moodLabel.width - Metrics.gap24
                        Text { width: parent.width; text: modelData.title; elide: Text.ElideRight; font.pointSize: Type.body; color: Theme.color.textPrimary }
                        Text { width: parent.width; text: modelData.artist + (modelData.album ? " · " + modelData.album : ""); elide: Text.ElideRight; font.pointSize: Type.meta; color: Theme.color.textSecondary }
                    }
                    Text {
                        id: standardLabel
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.standard ? I18n.tr("Analyzed", I18n.rev) : I18n.tr("Pending", I18n.rev)
                        font.pointSize: Type.meta
                        color: modelData.standard ? Theme.color.textSecondary : Theme.color.textTertiary
                    }
                    Text {
                        id: moodLabel
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.mood ? I18n.tr("Mood stored", I18n.rev) : I18n.tr("Mood pending", I18n.rev)
                        font.pointSize: Type.meta
                        color: modelData.mood ? Theme.color.textSecondary : Theme.color.textTertiary
                    }
                }
            }
        }
        Row {
            id: footer
            anchors.bottom: parent.bottom
            anchors.right: parent.right
            spacing: Metrics.gap8
            PillButton { text: I18n.tr("Refresh", I18n.rev); onClicked: dialog.fetch(true) }
            PillButton { text: I18n.tr("Load more", I18n.rev); enabled: dialog.tracks.length < dialog.summary.matched && dialog.error === ""; onClicked: dialog.fetch(false) }
        }
    }
}
