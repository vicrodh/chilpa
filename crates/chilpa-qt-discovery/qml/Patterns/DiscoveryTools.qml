import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

ScreenHead {
    id: tools
    property alias text: find.text
    property string mode: "genres"
    property string order: "name"
    property string scope: "all"
    property bool listing: true
    property bool recording: false
    property bool tracks: false
    property bool overview: false
    property bool work: false
    property bool detail: false
    signal queryChanged()
    flexible: find
    SearchField {
        id: find
        objectName: "discoverySearch"
        maximumLength: Metrics.discoverySearchChars
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, Math.min(Metrics.screenSearch, tools.flexibleRoom))
        visible: !tools.recording && !tools.tracks
        placeholder: tools.work ? I18n.tr("Filter recordings", I18n.rev) : I18n.tr("Search your library", I18n.rev)
        onTextChanged: tools.queryChanged()
        onEscaped: text = ""
    }
    SortButton {
        objectName: "discoverySort"
        anchors.verticalCenter: parent.verticalCenter
        visible: find.visible
        value: tools.order
        options: [{value:"name",text:I18n.tr("Name", I18n.rev)}].concat(tools.mode === "composers" && tools.listing && !tools.detail
            ? [{value:"albums",text:I18n.tr("Most albums", I18n.rev),fixed:true}] : [])
        onPicked: value => { tools.order = value; tools.queryChanged() }
    }
    SortButton {
        objectName: "discoveryFilter"
        anchors.verticalCenter: parent.verticalCenter
        visible: tools.mode === "genres" && (tools.listing || tools.overview)
        directed: false; glyph: Icons.filter; value: tools.scope
        options: [{value:"all",text:I18n.tr("All families", I18n.rev)},
            {value:"library",text:I18n.tr("In my library", I18n.rev)},
            {value:"tags",text:I18n.tr("My genre tags", I18n.rev)}]
        onPicked: value => { tools.scope = value; tools.queryChanged() }
    }
}
