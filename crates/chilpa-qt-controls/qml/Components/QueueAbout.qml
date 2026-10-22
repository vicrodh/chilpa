import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What is known of what plays, under the queue's art: the track by default,
// or the record it is on. Its source's word when it says much, MusicBrainz's
// when it does not; who said it closes the list.
Item {
    id: about
    // "track" or "album".
    property string part: "track"
    readonly property var found: read(part === "album" ? NowAbout.album(Player.path, Library.remoteRev) : NowAbout.track(Player.path, Library.remoteRev))
    readonly property var facts: found.facts || []
    readonly property var credits: found.credits || []
    readonly property string description: found.description || ""
    function read(text) { try { return JSON.parse(text || "{}") } catch (e) { return ({}) } }

    SegmentedControl {
        id: parts
        objectName: "queueAboutParts"
        options: [{text: I18n.tr("Track", I18n.rev)}, {text: I18n.tr("Album", I18n.rev)}]
        current: about.part === "album" ? 1 : 0
        onActivated: (index) => about.part = index === 1 ? "album" : "track"
    }

    Flickable {
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
            width: parent.width
            spacing: Metrics.gap10

            Text {
                visible: about.found.loading === true
                text: I18n.tr("Loading…", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            Text {
                objectName: "queueAboutDescription"
                visible: about.description !== ""
                width: parent.width
                text: about.description
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                font.pointSize: Type.small
                color: Theme.color.textSecondary
            }
            Repeater {
                objectName: "queueAboutFacts"
                model: about.facts
                Column {
                    required property var modelData
                    width: column.width
                    Text { text: modelData.label; font.pointSize: Type.meta; color: Theme.color.textTertiary }
                    TextEdit {
                        width: parent.width
                        text: modelData.value
                        readOnly: true; selectByMouse: true; wrapMode: TextEdit.Wrap
                        selectionColor: Theme.color.accent
                        font.pointSize: Type.small
                        color: Theme.color.textPrimary
                    }
                }
            }
            Text {
                visible: about.credits.length > 0
                topPadding: Metrics.gap4
                text: I18n.tr("Credits", I18n.rev)
                font.pointSize: Type.meta
                font.weight: Type.semibold
                color: Theme.color.textTertiary
            }
            Repeater {
                objectName: "queueAboutCredits"
                model: about.credits
                Column {
                    required property var modelData
                    width: column.width
                    Text { width: parent.width; text: modelData.name; textFormat: Text.PlainText; elide: Text.ElideRight; font.pointSize: Type.small; color: Theme.color.textPrimary }
                    Text { width: parent.width; visible: text !== ""; text: modelData.roles || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pointSize: Type.meta; color: Theme.color.textSecondary }
                }
            }
            Text {
                objectName: "queueAboutNothing"
                visible: about.found.loading === false && about.facts.length === 0 && about.credits.length === 0 && about.description === ""
                width: parent.width
                wrapMode: Text.Wrap
                text: I18n.tr("Nothing more is known about it.", I18n.rev)
                font.pointSize: Type.small
                color: Theme.color.textTertiary
            }
            Row {
                visible: (about.found.origin || "") !== ""
                spacing: Metrics.gap6
                Text { anchors.verticalCenter: parent.verticalCenter; text: I18n.tr("Source", I18n.rev) + " · " + (about.found.origin || ""); font.pointSize: Type.meta; color: Theme.color.textTertiary }
                PillButton {
                    objectName: "queueAboutLink"
                    visible: (about.found.link || "") !== ""
                    kind: "ghost"
                    text: I18n.tr("Open in browser", I18n.rev)
                    onClicked: Qt.openUrlExternally(about.found.link)
                }
            }
        }
    }
}
