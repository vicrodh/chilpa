import QtQuick
import QtQuick.Controls.Basic
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/qml/Screens"

ApplicationWindow {
    id: window
    width: 1280; height: 800; visible: true
    property string captureFolder: ""
    property bool captured: false
    Loader { id: home; anchors.fill: parent; sourceComponent: Component { HomeScreen {} } }
    Timer { interval: 150; running: true; repeat: true; onTriggered: Sources.refresh() }
    TestCase {
        id: tests
        name: "NativeHomeModules"
        when: window.visible
        function click(item) {
            verify(!!item, "the native control exists")
            waitForRendering(item)
            mouseClick(item, item.width / 2, item.height / 2)
        }
        function choose(source) {
            const dialog = findChild(home.item, "homeCustomizer")
            const tab = findChild(dialog.contentItem, "customizeGroup_" + source)
            click(tab)
            tryCompare(dialog, "activeGroup", source)
            tryVerify(() => home.item.homeSections.filter(section => section.sourceId === source).length === 6, 6000)
            const key = "source:" + source + ":shelf-0"
            tryVerify(() => findChild(dialog.contentItem, "sourceModuleToggle_" + key) !== null)
            const toggle = findChild(dialog.contentItem, "sourceModuleToggle_" + key)
            verify(!toggle.checked, "inspecting an opted-out source must not enable its modules")
            const tabs = findChild(dialog.contentItem, "customizeGroups")
            const left = findChild(dialog.contentItem, "customizeSourceChoices")
            const right = findChild(dialog.contentItem, "customizeActiveModules")
            verify(tabs.width > left.width + right.width)
            verify(tabs.mapToItem(dialog.contentItem, 0, tabs.height).y < left.mapToItem(dialog.contentItem, 0, 0).y)
            verify(tabs.mapToItem(dialog.contentItem, 0, tabs.height).y < right.mapToItem(dialog.contentItem, 0, 0).y)
            click(toggle)
            tryVerify(() => home.item.sections.some(section => section.id === key && section.enabled))
        }
        function test_opted_out_sources_offer_modules_and_keep_individual_choices() {
            tryVerify(() => JSON.parse(Sources.homeLibraries(0)).length === 2, 6000)
            verify(JSON.parse(Sources.homeLibraries(0)).every(source => !source.enabled))
            click(findChild(home.item, "homeCustomize"))
            tryVerify(() => findChild(home.item, "homeCustomizer")?.opened === true)
            choose("app.test.navigation")
            choose("app.test.second")
            const expected = ["source:app.test.navigation:shelf-0", "source:app.test.second:shelf-0"]
            const selected = () => home.item.sections.filter(section => section.id.startsWith("source:") && section.enabled).map(section => section.id)
            compare(JSON.stringify(selected().sort()), JSON.stringify(expected.slice().sort()))
            const dialog = findChild(home.item, "homeCustomizer")
            dialog.close()
            home.active = false
            wait(50)
            home.active = true
            tryVerify(() => selected().length === 2, 6000)
            compare(JSON.stringify(selected().sort()), JSON.stringify(expected.slice().sort()))
            verify(JSON.parse(Sources.homeLibraries(0)).every(source => !source.enabled), "individual choices preserve the default integration preference")
            wait(1500)
            verify(home.item.grabToImage(result => {
                verify(result.saveToFile(window.captureFolder + "/home-modules.png"))
                window.captured = true
            }))
            tryCompare(window, "captured", true)
        }
        function cleanupTestCase() {
            console.log(qtest_results.failCount ? "SMOKE FAIL native Home modules assertions=" + qtest_results.failCount
                : "SMOKE native Home modules completed: two opted-out sources; native pages; full-width tabs; individual choices; remount persistence")
            Qt.exit(qtest_results.failCount ? 1 : 0)
        }
    }
    Timer { interval: 30000; running: true; onTriggered: { console.log("SMOKE FAIL native Home modules timed out"); Qt.exit(1) } }
}
