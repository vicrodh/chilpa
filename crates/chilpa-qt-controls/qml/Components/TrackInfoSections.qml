import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What is known of a track in the information modal: the track, or the
// record it is on, in sections laid out in columns. Where its file or its
// server entry is is said here, and not in the queue's Info tab.
Item {
    id: body
    // "track" or "album".
    property string part: "track"
    // The track it speaks of, by the path that names it; empty for what plays.
    property string subject: ""
    readonly property string asked: subject !== "" ? subject : Player.path
    property int retryRevision: 0
    readonly property var found: visible ? read(part === "album" ? NowAbout.album(asked, Library.remoteRev + retryRevision) : NowAbout.track(asked, Library.remoteRev + retryRevision)) : ({})
    readonly property var facts: found.facts || []
    readonly property var technical: part === "track" ? found.technical || [] : []
    readonly property var credits: found.credits || []
    readonly property var places: part === "track" && found.file && found.file.value ? [{label: found.file.title || "", value: found.file.value, note: found.file.note || ""}] : []
    readonly property string review: found.description || ""
    readonly property bool known: facts.length + technical.length + credits.length + places.length > 0 || review !== ""
    readonly property bool failed: found.failed === true || found.busy === true
    function read(text) { try { return JSON.parse(text || "{}") } catch (e) { return ({}) } }
    function group(name) { return facts.filter(fact => (fact.group || "details") === name) }

    SegmentedControl {
        id: parts
        objectName: "trackInfoParts"
        options: [{text: I18n.tr("Track", I18n.rev)}, {text: I18n.tr("Album", I18n.rev)}]
        current: body.part === "album" ? 1 : 0
        onActivated: (index) => body.part = index === 1 ? "album" : "track"
    }
    IconButton {
        objectName: "trackInfoRefetch"
        anchors.right: parent.right
        anchors.verticalCenter: parts.verticalCenter
        visible: body.part === "album" && body.found.can_refetch === true
        enabled: body.found.loading !== true
        tip: I18n.tr("Re-fetch album information", I18n.rev)
        glyph: Lucide.rotateCcw
        iconFamily: Lucide.font
        onClicked: if (NowAbout.refetchAlbum(body.asked)) body.retryRevision++
    }

    Flickable {
        id: scroll
        objectName: "trackInfoScroll"
        anchors.top: parts.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        width: parent.width
        contentWidth: width
        contentHeight: column.height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar { gutter: -Metrics.gap4 }

        Column {
            id: column
            width: scroll.width
            spacing: Metrics.gap16

            Text {
                visible: body.found.loading === true
                text: I18n.tr("Loading…", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            InfoSection { objectName: "trackInfoTechnical"; width: parent.width; title: I18n.tr("Technical data", I18n.rev); cells: body.technical }
            InfoSection { objectName: "trackInfoDetails"; width: parent.width; title: I18n.tr("Details", I18n.rev); cells: body.group("details") }
            InfoSection { objectName: "trackInfoCredits"; width: parent.width; title: I18n.tr("Credits", I18n.rev); cells: body.credits; credits: true }
            InfoSection { objectName: "trackInfoLegal"; width: parent.width; title: I18n.tr("Legal data", I18n.rev); cells: body.group("legal") }
            InfoSection {
                objectName: "trackInfoReview"
                width: parent.width
                title: I18n.tr("Album review", I18n.rev)
                cells: body.review !== "" ? [{value: body.review}] : []
                maxColumns: 1
            }
            InfoSection { objectName: "trackInfoMedia"; width: parent.width; title: I18n.tr("Media info", I18n.rev); cells: body.places; code: true }
            Text {
                objectName: "trackInfoNothing"
                visible: body.found.loading === false && !body.known && !body.failed
                width: parent.width
                wrapMode: Text.Wrap
                text: I18n.tr("Nothing more is known about it.", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            Text {
                objectName: "trackInfoFailure"
                visible: body.failed
                width: parent.width
                wrapMode: Text.Wrap
                text: I18n.tr("Information could not be retrieved. Try again shortly.", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            PillButton {
                objectName: "trackInfoRetry"
                visible: body.failed
                text: I18n.tr("Retry", I18n.rev)
                glyph: Lucide.rotateCcw
                iconFamily: Lucide.font
                onClicked: if (NowAbout.retry(body.part, body.asked)) body.retryRevision++
            }
            Row {
                visible: (body.found.origin || "") !== ""
                width: parent.width
                spacing: Metrics.gap6
                Text {
                    width: parent.width - (browser.visible ? browser.width + parent.spacing : 0)
                    anchors.verticalCenter: parent.verticalCenter
                    text: I18n.tr("Source", I18n.rev) + " · " + (body.found.origin || "")
                    elide: Text.ElideRight
                    font.pointSize: Type.meta
                    color: Theme.color.textTertiary
                }
                IconButton {
                    id: browser
                    objectName: "trackInfoLink"
                    visible: (body.found.link || "") !== ""
                    tip: I18n.tr("Open in browser", I18n.rev)
                    glyph: Lucide.externalLink
                    iconFamily: Lucide.font
                    onClicked: Qt.openUrlExternally(body.found.link)
                }
            }
        }
    }
}
