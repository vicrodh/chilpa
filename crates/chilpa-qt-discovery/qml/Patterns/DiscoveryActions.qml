import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Column {
    id: root
    property var selected: ({})
    property var about: ({})
    property var choices: []
    property int tab: 0
    property bool overview: false
    property bool loading: false
    readonly property bool detail: !!selected.kind
    readonly property bool work: selected.kind === "work"
    readonly property bool genre: selected.kind === "genre"
    readonly property bool composer: selected.kind === "composer"
    signal openRequested(var entry)
    signal tabRequested(int tab)
    spacing: Metrics.gap12
    Flow {
        width: parent.width; spacing: Metrics.gap8
        visible: root.work && (root.selected.composers || []).length > 0
        Repeater { model: root.selected.composers || []; PillButton { required property string modelData; text: modelData; maximumWidth: parent.width; onClicked: root.openRequested({kind:"composer",name:modelData}) } }
    }
    Flow {
        objectName: "discoverySubgenres"
        width: parent.width; spacing: Metrics.gap8
        visible: root.genre && !root.overview && root.tab !== 4 && !root.loading
        Repeater { model: (root.about.items || []).slice(0, Metrics.discoveryPreview); PillButton { required property var modelData; text: modelData.name; maximumWidth: parent.width; onClicked: root.openRequested(modelData) } }
    }
    Row {
        visible: root.genre && !root.overview && root.tab !== 4 && (root.about.total || 0) > Metrics.discoveryPreview
        PillButton { text: I18n.tr("All subgenres", I18n.rev); kind: "ghost"; onClicked: { root.tabRequested(4) } }
    }
    Flow {
        width: parent.width
        visible: !!root.selected.recording
        spacing: Metrics.gap8
        PillButton { text: I18n.tr("All recordings", I18n.rev); kind: "ghost"; onClicked: Navigation.goKey("compositions", JSON.stringify(Object.assign({}, root.selected, {recording:"",recordingTitle:"",performer:"",album:0}))) }
        PillButton { objectName: "discoveryRecordingAlbum"; maximumWidth: parent.width; text: root.selected.recordingTitle || ""; visible: (root.selected.album || 0) !== 0; onClicked: Navigation.go("album", root.selected.album) }
        Text { width: Math.min(implicitWidth, parent.width); elide: Text.ElideRight; text: root.selected.performer || ""; textFormat: Text.PlainText; font.pointSize: Type.body; color: Theme.color.textSecondary }
    }
    SegmentedControl {
        objectName: "discoveryTabs"
        visible: root.detail && !root.selected.recording
        options: root.choices.map(text => ({text:text}))
        current: root.tab
        onActivated: index => root.tabRequested(index)
    }
    Row {
        spacing: Metrics.gap8
        visible: root.composer
        PillButton {
            text: I18n.tr("About this composer", I18n.rev); kind: "ghost"
            onClicked: {
                Visit.keep("artist.aboutIntent", {name:root.selected.name || "", at:Date.now()})
                Navigation.openArtist(root.selected.name || "")
            }
        }
    }
}
