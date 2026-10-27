import QtQuick
import QtTest
import "../../qml/Components"

TestCase {
    id: test
    name: "MetadataAdvisory"
    when: windowShown
    visible: true
    width: 600; height: 200
    MetadataField { id: field; width: test.width; ofTrack: true; field: ({name:"explicit",value:"",editable:true}) }
    SignalSpy { id: edits; target: field; signalName: "edited" }
    function init() { field.field = {name:"explicit",value:"",editable:true}; edits.clear() }
    function test_unknown_clean_and_explicit_remain_distinct() {
        const control = findChild(field, "metadataAdvisory")
        compare(control.current, 0)
        for (const choice of [{value:"false",index:1},{value:"true",index:2},{value:"",index:0}]) {
            control.picked(choice.value)
            compare(edits.signalArguments[edits.count - 1][0], "explicit")
            compare(edits.signalArguments[edits.count - 1][1], choice.value)
            field.field = {name:"explicit",value:choice.value,editable:true}
            compare(control.current, choice.index)
        }
    }
    function test_read_only_advisory_cannot_be_changed_by_keyboard() {
        field.field = {name:"explicit",value:"true",editable:false}
        const control = findChild(field, "metadataAdvisory")
        verify(!control.enabled)
        control.forceActiveFocus()
        keyClick(Qt.Key_Up)
        compare(edits.count, 0)
        compare(control.current, 2)
    }
}
