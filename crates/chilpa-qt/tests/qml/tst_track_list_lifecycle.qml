import QtQuick
import QtTest
import app.chilpa
import "../../qml/Components"

TestCase {
    id: test
    name: "TrackListLifecycle"
    when: windowShown
    visible: true
    width: 1000; height: 400
    Component {
        id: maker
        Loader {
            sourceComponent: TrackList {
                width: 1000; height: 300
                model: TrackListModel { kind: "source" }
            }
        }
    }
    function init() {
        failOnWarning(/invalid context|TypeError|exception occurred during delayed function evaluation/)
    }
    function test_a_reset_cannot_restore_a_retired_list() {
        for (let index = 0; index < 12; index++) {
            const scope = createTemporaryObject(maker, test)
            verify(scope.item !== null)
            scope.item.model.modelReset()
            scope.active = false
            wait(1)
            scope.destroy()
        }
        wait(30)
    }
}
