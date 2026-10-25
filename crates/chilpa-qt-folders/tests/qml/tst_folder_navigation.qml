import QtQuick
import QtTest
import app.chilpa
import "../../qml/Screens"
import "../../../chilpa-qt-kit/qml/Patterns"
import "../../../chilpa-qt-kit/qml/Theme"

TestCase {
    name: "FolderNavigation"
    when: windowShown
    visible: true
    width: 1280
    height: 720

    PageTransition { id: pages; anchors.fill: parent; retainedScreen: "folders" }
    Component { id: folders; FoldersScreen { routeKey: pages.routeKeyOf(parent) } }

    function test_folder_navigation_reuses_the_rail_and_resets_only_the_content_viewport() {
        SettingsModel.preferences = ({"ui.folders.mode":"tree"})
        Motion.reduced = true
        pages.sourceComponent = folders
        pages.route = "0\u001ffolders\u001f0\u001f/music/first"
        tryCompare(pages, "committedRoute", pages.route)
        const shell = pages.item
        const rail = findChild(shell, "folderNavigator")
        const list = findChild(rail, "folderNavigatorList")
        const rows = findChild(rail, "folderNavigatorRows")
        rail.selecting = true
        rail.choose("/music/chosen")
        for (let index = 0; index < 60; index++)
            rows.append({name: "Folder " + index, path: "/music/" + index, count: 1, art: "", folders: 0, album: 0, depth: 0, open: false, file: false})
        list.forceLayout()
        tryVerify(() => list.contentHeight > list.height)
        list.contentY = 300
        const contents = findChild(shell, "folderDetailContents")
        contents.contentY = 20
        pages.route = "0\u001ffolders\u001f0\u001f/music/second"
        compare(pages.item, shell)
        tryCompare(pages, "committedRoute", pages.route)
        compare(findChild(pages.item, "folderNavigator"), rail)
        verify(rail.selecting && rail.chosen["/music/chosen"])
        compare(list.contentY, 300, "the rail keeps its own viewport")
        compare(contents.contentY, 0, "only the folder contents start at the top")
        compare(findChild(shell, "folderContents").path, "/music/second")
        pages.route = "0\u001ffolders\u001f0\u001f/music/first"
        tryCompare(pages, "committedRoute", pages.route)
        compare(findChild(pages.item, "folderNavigator"), rail)
        compare(list.contentY, 300)
    }
}
