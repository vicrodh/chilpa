import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import com.blitzfc.chilpa.sources
import "../Theme"
import "../Primitives"
import "../Components"

// Temporary media has its own page and never changes library membership.
Item {
    id: page
    readonly property var document: JSON.parse(OpenMedia.snapshot || "{}")
    Component.onCompleted: OpenMedia.refresh()
    Timer { interval: Motion.listenRefresh; running: page.visible; repeat: true; onTriggered: OpenMedia.refresh() }
    TrackListModel {
        id: tracks
        kind: "openMedia"
        filterRev: OpenMedia.revision
        key: find.text
        playingPath: Player.path
    }
    Column {
        id: heading
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: side padding
        spacing: Metrics.gap12
        Text {
            text: I18n.tr("Open media", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            color: Theme.color.textPrimary
        }
        Flow {
            width: parent.width
            spacing: Metrics.gap8
            PillButton { objectName: "openFolder"; text: I18n.tr("Open a folder…", I18n.rev); glyph: Icons.folderOpen; enabled: !OpenMedia.loading; onClicked: OpenMedia.pick("folder") }
            PillButton { objectName: "openCd"; text: I18n.tr("Open audio CD", I18n.rev); glyph: Icons.disc; enabled: !OpenMedia.loading; onClicked: OpenMedia.pick("cd") }
            PillButton { objectName: "openSacd"; text: I18n.tr("Open SACD…", I18n.rev); glyph: Icons.disc; enabled: !OpenMedia.loading; onClicked: OpenMedia.pick("sacd") }
        }
        Text {
            width: parent.width
            visible: OpenMedia.loading || OpenMedia.error !== "" || !page.document.title
            text: OpenMedia.loading ? I18n.tr("Loading…", I18n.rev) : OpenMedia.error || I18n.tr("Open media without adding it to your library.", I18n.rev)
            wrapMode: Text.Wrap
            color: Theme.color.textSecondary
            font.pointSize: Type.body
        }
    }
    Flickable {
        anchors.top: heading.bottom
        anchors.topMargin: Metrics.gap16
        anchors.bottom: parent.bottom
        x: heading.x
        width: heading.width
        contentWidth: width
        contentHeight: body.height + Metrics.contentBottom
        clip: true
        visible: !!page.document.title
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ViewportScrollBar {}
        Column {
            id: body
            width: parent.width
            spacing: Metrics.gap16
            Item {
                width: parent.width
                height: hero.implicitHeight
                PageHero {
                    id: hero
                    width: parent.width - close.width - Metrics.gap12
                    name: page.document.title || ""
                    label: page.document.artist || ""
                    art: OpenMedia.art
                    meta: I18n.trCount("{} track", "{} tracks", page.document.count || 0, I18n.rev)
                    canShuffle: false
                    onPlay: tracks.playAll(false)
                }
                IconButton {
                    id: close
                    objectName: "closeMedia"
                    anchors.right: parent.right
                    glyph: Icons.x
                    tip: I18n.tr("Close media", I18n.rev)
                    onClicked: { OpenMedia.close(); ListenList.refresh(); Navigation.go("openMedia", 0) }
                }
            }
            SearchField {
                id: find
                width: Math.min(parent.width, Metrics.screenSearch)
                placeholder: I18n.tr("Filter tracks", I18n.rev)
                onEscaped: text = ""
            }
            TrackList {
                width: parent.width
                height: implicitHeight
                fitContent: true
                header: false
                rich: true
                showAlbum: false
                showArt: false
                showSource: false
                model: tracks
            }
        }
    }
}
