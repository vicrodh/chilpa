import QtQuick
import QtQuick.Controls.Basic
import com.blitzfc.chilpa
import "../Theme"
import "../Primitives"
import "../Components"

// Folders: the library as it sits on disk, one folder at a time, and any
// folder opened from outside it, read as it is. Playing a folder plays
// everything under it. The subfolders are the list; the folder's own tracks
// follow them.
ListView {
    id: page

    FolderModel {
        id: current
        path: Navigation.routeKey
    }
    TrackListModel {
        id: tracks
        kind: "folder"
        filterRev: current.rev
        playingPath: Player.path
    }

    // A vertical ListView places its rows at x = 0, so the page's margins
    // are the view's own.
    leftMargin: Metrics.contentX
    rightMargin: Metrics.contentX
    clip: true
    model: current
    reuseItems: true
    bottomMargin: Metrics.contentBottom
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}

    header: FolderHead {
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        folder: current
    }

    delegate: FolderEntryRow {
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        onOpen: Navigation.openFolder(path)
    }

    footer: Item {
        width: page.width - Metrics.contentX * 2 // derived: padding on both sides
        height: list.visible ? list.height + Metrics.section : 0

        TrackList {
            id: list
            y: Metrics.section
            width: parent.width
            height: implicitHeight
            visible: tracks.count > 0
            fitContent: true
            showArt: false
            model: tracks
        }
    }
}
