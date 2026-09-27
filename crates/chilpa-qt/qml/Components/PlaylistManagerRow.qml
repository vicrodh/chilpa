import QtQuick
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"

// One folder or playlist in either manager presentation.
Rectangle {
    id: row

    required property int index
    required property int rowKind
    required property int entryId
    required property string name
    required property string detail
    required property string source
    required property string sourceName
    required property url artwork
    required property url cover1
    required property url cover2
    required property url cover3
    required property url cover4
    required property string colorName
    required property int tracks
    required property int plays
    required property int folderId
    required property bool local
    required property bool visibleInSidebar

    property var manager
    property bool grid: true
    property bool custom: false
    signal editFolder(int id, string name, string colorName)
    signal editPlaylist(int id)

    readonly property bool folder: rowKind === 1
    readonly property color folderColor: !SettingsModel.folderColors ? Theme.color.textSecondary
        : colorName === "positive" ? Theme.color.statusPositive
        : colorName === "warning" ? Theme.color.statusWarning
        : colorName === "negative" ? Theme.color.statusNegative
        : Theme.color.accent

    implicitHeight: grid ? Metrics.playlistManagerCard : Metrics.playlistManagerListRow
    radius: Metrics.radiusCard
    color: "transparent"
    border.width: grid ? 1 : 0
    border.color: Theme.color.strokeHairline

    ListRowSurface {
        anchors.fill: parent
        index: row.index
        listMode: !row.grid
        hovered: hover.hovered
        restColor: row.grid ? Theme.color.surfacePanel : "transparent"
        radius: row.radius
    }

    Icon {
        visible: row.folder
        anchors.horizontalCenter: row.grid ? parent.horizontalCenter : undefined
        x: row.grid ? 0 : Metrics.gap12
        y: row.grid ? Metrics.gap16 : (parent.height - height) / 2
        glyph: Icons.folder
        size: row.grid ? Metrics.playlistManagerFolderIcon : Metrics.iconLg
        color: row.folderColor
    }

    ArtTile {
        id: art
        visible: !row.folder
        width: row.grid ? Metrics.playlistManagerArt : Metrics.folderArt
        height: width
        x: row.grid ? (parent.width - width) / 2 : Metrics.gap12
        y: row.grid ? Metrics.gap12 : (parent.height - height) / 2
        radius: Metrics.radiusArtSmall
        source: row.artwork.toString() !== "" ? row.artwork : row.cover1
        tint: row.index
    }

    Column {
        anchors.left: row.grid ? parent.left : art.right
        anchors.leftMargin: row.grid ? Metrics.gap10 : Metrics.gap12
        anchors.right: controls.left
        anchors.rightMargin: Metrics.gap8
        y: row.grid ? Metrics.playlistManagerTextY : (parent.height - height) / 2
        spacing: Metrics.gap2

        Text {
            width: parent.width
            text: row.name
            elide: Text.ElideRight
            horizontalAlignment: row.grid ? Text.AlignHCenter : Text.AlignLeft
            font.pointSize: Type.body
            font.weight: Type.semibold
            color: Theme.color.textPrimary
        }
        Text {
            width: parent.width
            text: row.folder
                ? I18n.trCount("{} playlist", "{} playlists", row.tracks, I18n.rev)
                : row.sourceName + " · " + I18n.trCount("{} track", "{} tracks", row.tracks, I18n.rev)
            elide: Text.ElideRight
            horizontalAlignment: row.grid ? Text.AlignHCenter : Text.AlignLeft
            font.pointSize: Type.small
            color: Theme.color.textTertiary
        }
    }

    Row {
        id: controls
        anchors.right: parent.right
        anchors.rightMargin: Metrics.gap8
        anchors.bottom: row.grid ? parent.bottom : undefined
        anchors.bottomMargin: row.grid ? Metrics.gap8 : 0
        anchors.verticalCenter: row.grid ? undefined : parent.verticalCenter
        spacing: Metrics.gap2

        IconButton {
            visible: row.custom && !row.folder
            size: Metrics.button24
            glyph: Icons.chevronUp
            tip: I18n.tr("Move up", I18n.rev)
            onClicked: row.manager.moveCustom(row.index, -1)
        }
        IconButton {
            visible: row.custom && !row.folder
            size: Metrics.button24
            glyph: Icons.chevronDown
            tip: I18n.tr("Move down", I18n.rev)
            onClicked: row.manager.moveCustom(row.index, 1)
        }
        IconButton {
            size: Metrics.button24
            glyph: Icons.dotsVertical
            tip: I18n.tr("More actions", I18n.rev)
            onClicked: actions.show()
        }
    }

    HoverHandler { id: hover }
    TapHandler { onDoubleTapped: row.manager.open(row.index) }

    Menu {
        id: actions
        property var actions: []

        function show() {
            const base = row.folder ? [
                {text: I18n.tr("Open", I18n.rev), act: "open"},
                {text: I18n.tr("Edit", I18n.rev), act: "edit"},
                {text: I18n.tr("Delete", I18n.rev), act: "delete"}
            ] : [
                {text: I18n.tr("Open", I18n.rev), act: "open"},
                {text: I18n.tr("Edit", I18n.rev), act: "edit-playlist", when: row.local},
                {text: row.visibleInSidebar ? I18n.tr("Hide from sidebar", I18n.rev)
                                                : I18n.tr("Show in sidebar", I18n.rev), act: "visible"},
                {separator: true},
                {text: I18n.tr("Move to root", I18n.rev), act: "folder", id: 0}
            ]
            if (!row.folder) {
                const ids = row.manager.folderIds()
                const names = row.manager.folderNames()
                for (let i = 0; i < ids.length; ++i)
                    base.push({text: names[i], act: "folder", id: Number(ids[i])})
                if (row.local)
                    base.push({separator: true}, {text: I18n.tr("Delete", I18n.rev), act: "delete"})
            }
            actions = base.filter(e => e.when === undefined || e.when)
            entries = actions
            open()
        }

        onChosen: (at) => {
            const action = actions[at]
            if (action.act === "open") row.manager.open(row.index)
            else if (action.act === "edit") row.editFolder(row.entryId, row.name, row.colorName)
            else if (action.act === "edit-playlist") row.editPlaylist(row.entryId)
            else if (action.act === "visible") row.manager.setVisible(row.entryId, !row.visibleInSidebar)
            else if (action.act === "folder") row.manager.moveToFolder(row.entryId, action.id)
            else if (action.act === "delete") {
                if (row.folder) row.manager.removeFolder(row.entryId)
                else Playlists.remove(row.entryId)
            }
        }
    }
}
