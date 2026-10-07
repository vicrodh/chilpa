import QtQuick
import QtTest
import app.chilpa
import app.chilpa.metadata
import "../../qml/Screens"

TestCase {
    name: "MetadataScreen"
    when: windowShown
    visible: true
    width: 1100
    height: 700

    Loader {
        id: host
        anchors.fill: parent
        active: false
        sourceComponent: MetadataScreen {}
    }

    function init() {
        host.active = false
        MetadataEditor.opened = []
        MetadataEditor.show({ state: "closed", album_id: 0, dirty: false, error: "" })
        Navigation.page = ""
    }

    function test_it_opens_the_record_of_its_route() {
        Navigation.screen = "metadata"
        Navigation.routeId = 7
        host.active = true
        compare(MetadataEditor.opened, [7])
    }

    function test_on_another_route_it_opens_nothing() {
        Navigation.screen = "album"
        Navigation.routeId = 7
        host.active = true
        compare(MetadataEditor.opened, [])
    }

    function test_a_save_that_lands_goes_to_the_record() {
        Navigation.screen = "metadata"
        Navigation.routeId = 7
        host.active = true
        MetadataEditor.show({ state: "saving", album_id: 7, dirty: true, error: "" })
        MetadataEditor.show({ state: "closed", album_id: 7, dirty: false, error: "" })
        compare(Navigation.page, "album")
        compare(Navigation.selectedId, 7)
    }
}
