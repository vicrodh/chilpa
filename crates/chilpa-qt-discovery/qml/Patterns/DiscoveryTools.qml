import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "../Components"

ScreenHead {
    id: tools
    objectName: "discoveryTools"
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
    property var sources: []
    property var excludedSources: []
    // Composers and compositions of the classical tradition only, as Roon
    // opens them; the other choice shows every one.
    property bool classical: true
    property bool composer: false
    readonly property bool classicalChoice: (mode === "compositions" || mode === "composers") && listing && !detail
    // What an empty listing says, for the search, the switch and the mode.
    readonly property string emptyText: text !== "" ? I18n.tr("No matches. Try another search.", I18n.rev)
        : mode === "genres" ? I18n.tr("No genres with albums in this selection", I18n.rev)
        : classicalChoice && classical ? (mode === "composers" ? I18n.tr("No classical composers yet. Choose All composers to see every composer credit.", I18n.rev)
            : I18n.tr("No classical compositions yet. Choose All compositions to see every work.", I18n.rev))
        : mode === "composers" && !composer ? I18n.tr("No composer credits yet. Add composers in the metadata editor.", I18n.rev)
        : I18n.tr("No work tags in this selection. Composer credits are still available under Albums and Tracks.", I18n.rev)
    signal sourcesChosen(var excluded)
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
    SegmentedControl {
        objectName: "discoveryClassical"
        anchors.verticalCenter: parent.verticalCenter
        visible: tools.classicalChoice
        options: [{text:I18n.tr("Classical", I18n.rev)}, {text:tools.mode === "composers" ? I18n.tr("All composers", I18n.rev) : I18n.tr("All compositions", I18n.rev)}]
        current: tools.classical ? 0 : 1
        onActivated: index => { tools.classical = index === 0; tools.queryChanged() }
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
    GenreSources {
        anchors.verticalCenter: parent.verticalCenter
        visible: tools.mode === "genres"
        sources: tools.sources; excluded: tools.excludedSources
        onChosen: excluded => tools.sourcesChosen(excluded)
    }
}
