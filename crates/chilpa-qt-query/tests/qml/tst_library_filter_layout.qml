import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/query/qml/Patterns"

TestCase {
    id: test
    name: "LibraryFilterLayout"
    when: windowShown
    visible: true
    width: 1000; height: 300
    LibraryPanes { id: panes; width: 1000; height: 300 }
    Component { id: columnMaker; PaneColumn { width: 500; height: 300 } }
    function test_technical_labels_preserve_model_keys_and_row_actions_data() {
        return [{tag:"bits",column:"bits",key:"16",label:"16-bit"},{tag:"rate",column:"rate",key:"44100",label:"44.1kHz"}]
    }
    function test_technical_labels_preserve_model_keys_and_row_actions(data) {
        failOnWarning(/ReferenceError|TypeError|Binding loop/)
        const pane = createTemporaryObject(columnMaker, test, {column:data.column})
        const list = Array.from(pane.children).find(child => child.contentItem !== undefined)
        list.model.append({key:"",label:"All",meta:"1",selected:true,excluded:false,all:true})
        list.model.append({key:data.key,label:data.key,meta:"1",selected:false,excluded:false,all:false})
        tryVerify(() => list.itemAtIndex(1) !== null)
        const row = list.itemAtIndex(1)
        compare(findChild(row, "paneLabel").text, data.label)
        compare(findChild(list.itemAtIndex(0), "paneLabel").text, "All")
        compare(row.key, data.key)
        mouseClick(row, 10, 5)
        compare(list.model.choices, [[1,0]])
        compare(list.model.get(1).key, data.key)
    }
    function test_a_single_filter_uses_the_full_width_data() {
        return ["year", "genre", "label", "composer", "rating", "bits", "rate", "artist", "album"].map(key => ({tag:key,column:key}))
    }
    function test_a_single_filter_uses_the_full_width(data) {
        failOnWarning(/ReferenceError|TypeError|invalid context/)
        panes.shown = [data.column]
        const widgets = () => Array.from(panes.children).reduce((items, child) => items.concat(Array.from(child.children || [])), [])
            .filter(child => child.column === data.column)
        tryVerify(() => widgets().length === 1)
        fuzzyCompare(widgets()[0].width, panes.width - 2, 0.1)
    }
}
