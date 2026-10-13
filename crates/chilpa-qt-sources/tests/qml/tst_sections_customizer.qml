import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import "../../qml/Patterns"

TestCase {
    id: test
    name: "SectionsCustomizer"
    when: windowShown
    visible: true
    width: 900
    height: 700

    property int backgroundClicks: 0
    MouseArea { anchors.fill: parent; onClicked: test.backgroundClicks++ }
    SectionsCustomizer {
        id: customizer
        grouped: true
        groupSources: [{id: "fixture", name: "Fixture", icon: ""}]
        sections: [{id: "local", title: "Local", enabled: true},
                   {id: "source", title: "Source", sourceId: "fixture", enabled: true}]
    }

    function cleanup() { customizer.close(); backgroundClicks = 0 }

    function test_source_tab_keeps_dialog_open_without_clicking_the_page() {
        customizer.open()
        tryVerify(() => customizer.opened)
        const tab = findChild(customizer.contentItem, "customizeGroup_fixture")
        verify(tab !== null)
        tryVerify(() => tab.x > 0, 1000, "the row places the source after Chilpa")
        verify(tab.width > 0 && tab.height > 0, "the source tab has a hit area")
        const center = tab.mapToItem(test, tab.width / 2, tab.height / 2)
        verify(center.x > 0 && center.x < test.width && center.y > 0 && center.y < test.height,
               "the tab is inside the window: " + center.x + ", " + center.y)
        verify(tab.visible && tab.enabled, "the tab accepts input")
        mouseClick(tab, tab.width / 2, tab.height / 2)
        compare(customizer.activeGroup, "fixture")
        verify(customizer.opened)
        compare(backgroundClicks, 0)
    }
}
