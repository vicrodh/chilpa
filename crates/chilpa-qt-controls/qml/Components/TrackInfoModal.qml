import QtQuick
import app.chilpa
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// What is known of a track: the one that plays, opened from its title in
// the player, or one a list's menu chose. The track, its record and its
// artist head it; the sections under them scroll inside a fixed size.
Modal {
    id: modal
    objectName: "trackInfoModal"
    preferredWidth: Metrics.trackInfoWidth + Metrics.modalInsets
    preferredHeight: Metrics.trackInfoHeight + Metrics.modalInsets
    // The chosen track's path and the title its row showed; empty for the
    // track that plays.
    property string subject: ""
    property string subjectTitle: ""
    // The track's own answer, whichever part the sections show: it names the
    // track and holds what the copy button takes.
    readonly property var track: visible ? sections.read(NowAbout.track(sections.asked, Library.remoteRev + sections.retryRevision)) : ({})
    readonly property var heading: track.heading || ({})

    Clipboard { id: clipboard; objectName: "trackInfoClipboard" }

    Column {
        width: parent.width
        spacing: Metrics.gap12
        Item {
            id: header
            width: parent.width
            height: Math.max(names.height, actions.height)
            Column {
                id: names
                width: parent.width - actions.width - Metrics.gap16
                spacing: Metrics.gap2
                Text {
                    objectName: "trackInfoTitle"
                    width: parent.width
                    text: modal.subject !== "" ? modal.heading.title || modal.subjectTitle : Player.title
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
                    font.pointSize: Type.section; font.weight: Type.semibold; color: Theme.color.textPrimary
                }
                Text {
                    objectName: "trackInfoAlbum"
                    width: parent.width
                    visible: text !== ""
                    text: modal.subject !== "" ? modal.heading.album || "" : Player.album
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
                    font.pointSize: Type.meta; color: Theme.color.textSecondary
                }
                Text {
                    objectName: "trackInfoArtist"
                    width: parent.width
                    visible: text !== ""
                    text: modal.subject !== "" ? modal.heading.artist || "" : Player.artist
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
                    font.pointSize: Type.small; color: Theme.color.textPrimary
                }
            }
            Row {
                id: actions
                anchors.right: parent.right
                spacing: Metrics.gap8
                IconButton {
                    objectName: "trackInfoCopy"
                    visible: (modal.track.copy_basic || "") !== ""
                    glyph: Icons.copy
                    tip: I18n.tr("Copy", I18n.rev)
                    onClicked: copyMenu.open()
                    Menu {
                        id: copyMenu
                        objectName: "trackInfoCopyMenu"
                        y: parent.height + Metrics.gap4
                        entries: [{text: I18n.tr("Copy track, album and artist", I18n.rev)}, {text: I18n.tr("Copy full information", I18n.rev)}]
                        onChosen: index => clipboard.write(index === 0 ? modal.track.copy_basic || "" : modal.track.copy_full || "")
                    }
                }
                IconButton { objectName: "trackInfoClose"; glyph: Icons.x; tip: I18n.tr("Close", I18n.rev); onClicked: modal.close() }
            }
        }
        TrackInfoSections {
            id: sections
            objectName: "trackInfoSections"
            subject: modal.subject
            width: parent.width
            height: Math.max(0, parent.parent.height - header.height - Metrics.gap12)
        }
    }
}
