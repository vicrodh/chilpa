import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Components"

// A genre's or composer's tracks, filtered from their column headers as
// Tracks is. The filters last as long as the page; a recording keeps the
// plain list.
Item {
    id: section
    property string key: ""
    property bool recording: false
    property bool autoPlay: false
    property bool showLegacyActions: true
    // The page the scrollbar and its hover margin belong to.
    property Item scrollbarOverlay: null
    readonly property alias collection: parts
    readonly property bool playable: parts.ready && parts.count > 0
    readonly property bool sheetFiltered: !recording && sheet.filtered
    // The selection, then (U+001E) a Tracks query holding only the column selections.
    readonly property string sheetKey: key + "\u001e" + ["", "", "", "", JSON.stringify(sheet.effective)].join("\u001f")
    signal played()
    function tryPlay() {
        if (!autoPlay || !parts.ready || parts.count === 0) return
        autoPlay = false; parts.playAll(false); played()
    }
    function filterColumn(field, label, anchor) { columnFilter.show(field, label, sheet.filters[field], anchor) }
    onAutoPlayChanged: Qt.callLater(tryPlay)
    Component.onCompleted: Qt.callLater(tryPlay)
    readonly property bool narrow: width < Metrics.discoveryTrackNarrow
    readonly property alias scrollView: list.rows
    readonly property bool ready: parts.ready
    TrackSheetState { id: sheet; settings: SettingsModel; visits: Visit; memory: "" }
    TrackListModel { id: parts; kind: "discovery"; key: section.sheetFiltered ? section.sheetKey : section.key; filterRev: Library.contentRev + Library.remoteRev; playingPath: Player.path }
    Connections { target:parts; function onReadyChanged() { section.tryPlay() } function onCountChanged() { section.tryPlay() } }
    Row {
        id: actions
        spacing: Metrics.gap8
        PillButton { visible: section.showLegacyActions; objectName:"discoveryPlay"; text: I18n.tr("Play", I18n.rev); kind: "primary"; enabled: parts.ready && parts.count > 0; onClicked: parts.playAll(false) }
        PillButton { visible: section.showLegacyActions; objectName:"discoveryQueue"; text: I18n.tr("Add to queue", I18n.rev); enabled: parts.ready && parts.count > 0; onClicked: parts.queueAll() }
        Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.trCount("{} track", "{} tracks", parts.count, I18n.rev); font.pointSize: Type.meta; color: Theme.color.textSecondary }
    }
    TrackFilterTrail {
        id: trail
        objectName: "discoveryFilterTrail"
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: actions.bottom
        anchors.topMargin: visible ? Metrics.gap10 : 0
        filters: section.recording ? ({}) : sheet.effective
        sourceNames: sheet.sourceNames
        onEdited: (field, label, anchor) => section.filterColumn(field, label, anchor)
        onRemoved: field => sheet.apply(field, null)
        onCleared: { columnFilter.close(); sheet.filters = ({}); sheet.sourceNames = ({}) }
    }
    TrackSheetHeader {
        id: head
        objectName: "discoveryTrackHeader"
        visible: !section.recording
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: trail.bottom; anchors.topMargin: Metrics.gap12
        cols: list.columns
        filters: sheet.effective
        onRequested: (field, label, anchor) => section.filterColumn(field, label, anchor)
    }
    TrackList {
        id: list
        objectName: "discoveryTrackList"
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        anchors.top: section.recording ? trail.bottom : head.bottom; anchors.topMargin: section.recording ? Metrics.gap12 : 0
        header: section.recording
        scrollbarOverlay: section.scrollbarOverlay
        model: parts
        enabled:parts.ready
        showArtist: !section.recording
        showAlbum: !section.recording && !section.narrow
        showArt: !section.recording
        showRating: !section.recording && !section.narrow
        showSource: !section.narrow
    }
    TrackColumnChoices { id: choices; key: section.sheetKey; filterRev: parts.filterRev }
    TrackColumnFilter {
        id: columnFilter
        model: choices
        contentViewport: section
        sourceNames: sheet.sourceNames
        onApplied: (field, values, labels) => sheet.apply(field, values, labels)
    }
    Text {
        anchors.centerIn: parent; visible: parts.ready && parts.count === 0
        text: section.sheetFiltered ? I18n.tr("No tracks match your search.", I18n.rev) : I18n.tr("No tracks in this selection", I18n.rev)
        font.pointSize: Type.body; color: Theme.color.textSecondary
    }
}
