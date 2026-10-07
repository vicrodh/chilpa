import QtQuick
import app.chilpa
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "../Components"

// Artists (design/06): who the records are credited to, or everyone who
// plays on a track, as round portraits in the shared grid. A portrait is a
// cover of one of their records.
Item {
    id: page

    property string role: SettingsModel.kept("ui.artists.role") === "all" ? "all" : "album"

    AlbumGridModel {
        id: everyone
        kind: "artists"
        key: page.role
        contentRev: Library.contentRev
        columns: grid.columns
        filter: find.text
    }

    Item {
        id: head
        x: Metrics.contentX
        y: Metrics.contentTop
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: Metrics.shelfHeader

        Text {
            id: title
            anchors.verticalCenter: parent.verticalCenter
            text: I18n.tr("Artists", I18n.rev)
            font.pointSize: Type.title
            font.weight: Type.bold
            font.letterSpacing: Type.titleTracking
            color: Theme.color.textPrimary
        }
        // How many there are, in the room the title and the tools leave.
        Text {
            anchors.left: title.right
            anchors.leftMargin: Metrics.gap12
            anchors.right: tools.left
            anchors.rightMargin: Metrics.gap12
            anchors.baseline: title.baseline
            text: I18n.trCount("{} artist", "{} artists", everyone.albums, I18n.rev)
            elide: Text.ElideRight
            font.pointSize: Type.meta
            color: Theme.color.textSecondary
        }
        Row {
            id: tools
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Metrics.gap12

            SearchField {
                id: find
                anchors.verticalCenter: parent.verticalCenter
                // What the title and the roles leave, and no less than it takes to type in.
                width: Math.max(Metrics.folderTreeMin, Math.min(Metrics.screenSearch, head.width - title.width - roles.width - Metrics.folderTreeMin))
                placeholder: I18n.tr("Filter artists", I18n.rev)
                onEscaped: text = ""
            }
            SegmentedControl {
                id: roles
                anchors.verticalCenter: parent.verticalCenter
                options: [
                    { text: I18n.tr("Album artists", I18n.rev), tip: I18n.tr("Who the records are credited to", I18n.rev) },
                    { text: I18n.tr("All artists", I18n.rev), tip: I18n.tr("Everyone who plays on a track", I18n.rev) }
                ]
                current: page.role === "all" ? 1 : 0
                onActivated: (index) => {
                    page.role = index === 1 ? "all" : "album"
                    SettingsModel.keep("ui.artists.role", page.role)
                }
            }
        }
    }

    AlbumGrid {
        id: grid
        anchors.top: head.bottom
        anchors.topMargin: Metrics.gap12
        anchors.bottom: parent.bottom
        x: head.x
        width: head.width
        round: true
        bottomInset: Metrics.contentBottom
        model: everyone
        onOpen: (albumId, title) => Navigation.openArtist(title)
    }
}
