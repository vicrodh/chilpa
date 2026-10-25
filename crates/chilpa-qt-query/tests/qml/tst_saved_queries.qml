import QtQuick
import QtTest
import app.chilpa
import app.chilpa.query
import "../../qml/Patterns"

TestCase {
    id: test
    name: "SavedQueries"
    when: windowShown
    visible: true
    width: 1100
    height: 850
    RefinePanel { id: docked; width: 520; height: 600; narrow: true }
    RefinePanel { id: detached; x: 540; width: 520; height: 600; detached: true }

    function loader(panel) { return findChild(panel, "savedQueriesLoader") }
    function dialog(panel, saving) {
        findChild(panel, saving ? "refineSaveQuery" : "refineSavedQueries").clicked()
        tryVerify(() => loader(panel).item !== null)
        tryVerify(() => loader(panel).item.opened)
        return loader(panel).item
    }
    function dismiss(panel) {
        if (loader(panel).item) loader(panel).item.close()
        tryVerify(() => loader(panel).item === null)
    }
    function init() {
        SavedQueries.savedEntries = []
        SavedQueries.rev = 0
        SavedQueries.catalog = '{"entries":[],"revision":0}'
        SavedQueries.savedError = ""
        SavedQueries.savedNextId = 1
        SavedQueries.savedLoads = 0
        SavedQueries.previewReads = 0
        SavedQueries.appliedQueries = 0
        Library.queryText = '(genre:jazz | title:"OR")'
    }
    function cleanup() { dismiss(docked); dismiss(detached) }

    function test_save_failure_keeps_name_expression_and_revision() {
        const text = Library.queryText
        const form = dialog(docked, true)
        const name = findChild(form, "savedQueryName")
        name.text = "Evening"
        SavedQueries.savedError = "Disk full"
        findChild(form, "savedQuerySubmit").clicked()
        compare(form.error, "Disk full")
        compare(name.text, "Evening")
        compare(form.expression, text)
        compare(SavedQueries.rev, 0)
        compare(Library.queryText, text)
        SavedQueries.savedError = ""
        name.accepted()
        tryVerify(() => loader(docked).item === null)
        compare(SavedQueries.savedEntries[0].expression, text)
        compare(SavedQueries.savedLoads, 1)
        compare(SavedQueries.appliedQueries, 0)
    }
    function test_table_uses_one_dialog_snapshot_and_keeps_shared_summary_small() {
        for (let n = 0; n < 64; n++) SavedQueries.save("Query " + n, "year:" + (1900 + n))
        verify(JSON.parse(SavedQueries.catalog).entries.every(entry => entry.expression === undefined))
        const form = dialog(docked, false)
        const list = findChild(form, "savedQueriesList")
        tryVerify(() => list.itemAtIndex(0) !== null)
        tryCompare(findChild(list.itemAtIndex(0), "savedQueryRowFilter"), "text", "year:1900")
        compare(SavedQueries.previewReads, 1, "one bounded read, not one read per row")
        list.positionViewAtEnd()
        wait(50)
        compare(SavedQueries.previewReads, 1, "scrolling never rereads storage")
        compare(SavedQueries.appliedQueries, 0)
        dismiss(docked)
        verify(loader(docked).item === null, "the dialog owns and releases the snapshot")
    }
    function test_both_panels_share_committed_entries_and_open_on_demand() {
        let form = dialog(docked, true)
        findChild(form, "savedQueryName").text = "First"
        form.submit()
        tryVerify(() => loader(docked).item === null)
        Library.queryText = "year:1990"
        form = dialog(detached, false)
        compare(form.entries.length, 1)
        compare(SavedQueries.appliedQueries, 0)
        form.rename(form.entries[0])
        findChild(form, "savedQueryName").text = "Renamed"
        form.submit()
        compare(form.entries[0].name, "Renamed")
        compare(Library.queryText, "year:1990")
        form.openEntry(form.entries[0].id)
        tryVerify(() => loader(detached).item === null)
        compare(Library.queryText, '(genre:jazz | title:"OR")')
        compare(SavedQueries.appliedQueries, 1)
    }
    function test_cancel_and_confirm_delete_preserve_the_active_filter() {
        SavedQueries.save("First", "genre:jazz")
        const before = Library.queryText
        const form = dialog(docked, false)
        form.askRemove(form.entries[0])
        const confirm = findChild(form, "savedQueryConfirmation")
        confirm.close()
        compare(SavedQueries.savedEntries.length, 1)
        form.askRemove(form.entries[0])
        confirm.answered = true
        confirm.close()
        confirm.confirmed()
        compare(SavedQueries.savedEntries.length, 0)
        compare(Library.queryText, before)
        compare(SavedQueries.appliedQueries, 0)
    }
    function test_empty_limit_and_repeated_open_close_do_not_apply_queries() {
        let form = dialog(docked, false)
        verify(findChild(form, "savedQueriesEmpty").visible)
        dismiss(docked)
        for (let n = 0; n < 64; n++) SavedQueries.save("Long query name " + n, "year:" + (1900 + n))
        for (let n = 0; n < 3; n++) {
            form = dialog(detached, false)
            compare(form.entries.length, 64)
            dismiss(detached)
        }
        compare(SavedQueries.appliedQueries, 0)
        compare(SavedQueries.savedLoads, 4)
    }
    function test_invalid_open_and_cancel_rename_leave_active_conditions() {
        SavedQueries.save("First", "genre:jazz")
        const before = Library.queryText
        const form = dialog(detached, false)
        SavedQueries.savedError = "Invalid query"
        form.openEntry(form.entries[0].id)
        compare(form.error, "Invalid query")
        compare(Library.queryText, before)
        SavedQueries.savedError = ""
        form.rename(form.entries[0])
        findChild(form, "savedQueryName").text = "Not saved"
        keyClick(Qt.Key_Escape)
        tryVerify(() => loader(detached).item === null)
        compare(SavedQueries.savedEntries[0].name, "First")
        compare(Library.queryText, before)
    }
}
