import QtQuick
import QtQuick.Effects
import QtTest
import app.chilpa.kit
import "../../qml/Patterns"
import "../../qml/Theme"

TestCase {
    id: test
    name: "PageLifetime"
    when: windowShown
    visible: true
    width: 500; height: 400
    property int built: 0
    PageLifetime { id: lifetime }
    Loader { id: owner; anchors.fill: parent }
    PageTransition { id: pages; anchors.fill: parent }
    Component { id: emptyPage; Item {} }
    property Item retiredMaterial: null
    Component {
        id: shadowPage
        RectangularShadow { width: 100; height: 40; radius: 6 }
    }
    function test_retired_shadow_releases_its_native_material() {
        for (let index = 0; index < 4; ++index) {
            owner.sourceComponent = shadowPage
            tryVerify(() => owner.item.children.length > 0)
            test.retiredMaterial = owner.item.children[0]
            verify(test.retiredMaterial !== null)
            lifetime.cancel(owner.item)
            owner.sourceComponent = null
            tryCompare(test, "retiredMaterial", null)
        }
    }
    Component { id: emptyShadow; RectangularShadow {} }
    function test_retired_shadow_without_a_window_releases_its_material() {
        for (const factory of [shadowPage, emptyShadow]) {
            // Closed popup content has not joined a window or initialized
            // the default shader's visual parent yet.
            const shadow = factory.createObject(null)
            compare(shadow.parent, null)
            compare(shadow.children.length, 0)
            test.retiredMaterial = shadow.material
            verify(test.retiredMaterial !== null)
            lifetime.cancel(shadow)
            shadow.destroy()
            tryCompare(test, "retiredMaterial", null)
        }
    }
    Component { id: customMaterial; ShaderEffect {} }
    function test_retiring_a_shadow_preserves_its_custom_material() {
        const material = customMaterial.createObject(null)
        const shadow = emptyShadow.createObject(null)
        shadow.material = material
        test.retiredMaterial = material
        lifetime.cancel(shadow)
        shadow.destroy()
        wait(1)
        verify(test.retiredMaterial !== null)
        material.destroy()
        tryCompare(test, "retiredMaterial", null)
    }
    Component {
        id: boundPage
        Item {
            property bool requested: true
            property alias body: body
            Loader {
                id: body
                active: parent.requested
                asynchronous: true
                sourceComponent: Item { Repeater { model: 300; delegate: Rectangle {} } }
            }
        }
    }
    ListModel { id: records; ListElement { title: "Retained record" } }
    Component {
        id: listPage
        ListView {
            objectName: "recordsView"
            model: records
            delegate: Item { width: 100; height: 30 }
        }
    }
    Component {
        id: nestedPage
        Item {
            Loader {
                objectName: "pendingBody"
                asynchronous: true
                sourceComponent: Item {
                    Repeater {
                        model: 300
                        delegate: Item {
                            Component.onCompleted: test.built++
                            Repeater { model: 4; delegate: Rectangle {} }
                        }
                    }
                }
            }
        }
    }
    property bool cancelledDelegate: false
    Component {
        id: repeatingPage
        Item {
            property alias delegates: repeats
            Repeater {
                id: repeats
                model: 1
                delegate: Loader {
                    asynchronous: true
                    onActiveChanged: if (!active) test.cancelledDelegate = true
                    sourceComponent: Item { Repeater { model: 300; delegate: Rectangle {} } }
                }
            }
        }
    }
    function test_repeater_loaders_are_cancelled_before_removal() {
        test.cancelledDelegate = false
        owner.sourceComponent = repeatingPage
        compare(owner.item.delegates.itemAt(0).status, Loader.Loading)
        lifetime.cancel(owner.item)
        verify(test.cancelledDelegate, "the delegate's own loader must be stopped before its context")
        owner.sourceComponent = null
        wait(50)
    }
    Component {
        id: listedPage
        ListView {
            model: 1
            width: 400; height: 300
            delegate: Loader {
                width: 400; height: 100
                asynchronous: true
                onActiveChanged: if (!active) test.cancelledDelegate = true
                sourceComponent: Item { Repeater { model: 300; delegate: Rectangle {} } }
            }
        }
    }
    function test_visual_view_delegates_are_cancelled_before_removal() {
        test.cancelledDelegate = false
        owner.sourceComponent = listedPage
        owner.item.forceLayout()
        const delegate = owner.item.itemAtIndex(0)
        verify(delegate !== null)
        compare(delegate.status, Loader.Loading)
        lifetime.cancel(owner.item)
        verify(test.cancelledDelegate)
        owner.sourceComponent = null
        wait(50)
    }
    function init() {
        failOnWarning(/Cannot create delegate|Object or context destroyed during incubation|invalid context|TypeError/)
        Motion.reduced = true
    }
    function test_retired_loader_bindings_cannot_restart_creation() {
        owner.sourceComponent = boundPage
        const page = owner.item, body = page.body
        lifetime.cancel(page)
        compare(body.active, false)
        page.requested = false
        page.requested = true
        compare(body.active, false, "a retired binding must not recreate its contents")
        owner.sourceComponent = null
        wait(30)
    }
    function test_departing_page_cancels_nested_creation() {
        for (let index = 0; index < 8; index++) {
            pages.sourceComponent = nestedPage
            pages.route = "nested-" + index
            tryCompare(pages, "phase", "content")
            // A ready page may still own an independently incubating child.
            pages.sourceComponent = emptyPage
            pages.route = "empty-" + index
            tryCompare(pages, "committedRoute", pages.route)
        }
        wait(50)
    }
    function test_shutdown_cancels_content_owned_outside_page_slots() {
        const extra = boundPage.createObject(pages)
        compare(extra.body.status, Loader.Loading)
        pages.releaseAll()
        compare(extra.body.active, false)
        extra.requested = false
        extra.requested = true
        compare(extra.body.active, false)
        extra.destroy()
        wait(30)
    }
    function test_nested_creation_is_cancelled_before_the_context() {
        for (let index = 0; index < 8; index++) {
            owner.sourceComponent = nestedPage
            const body = findChild(owner.item, "pendingBody")
            compare(body.status, Loader.Loading)
            const previousBuilds = test.built
            lifetime.cancel(owner.item)
            compare(body.status, Loader.Null)
            compare(body.asynchronous, true)
            compare(test.built, previousBuilds, "cancelling must not finish unused delegates")
            owner.sourceComponent = null
            wait(1)
        }
        wait(50)
    }
    function test_cancelling_a_view_preserves_its_records_and_can_reopen() {
        owner.sourceComponent = listPage
        compare(owner.item.count, 1)
        lifetime.cancel(owner.item)
        compare(owner.item.count, 0)
        compare(records.count, 1)
        owner.sourceComponent = null
        owner.sourceComponent = listPage
        compare(owner.item.count, 1)
        owner.sourceComponent = null
    }
}
