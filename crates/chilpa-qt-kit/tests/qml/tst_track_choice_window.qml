import QtQuick
import QtTest
import "../../qml/Components"

TestCase {
    id: test
    name: "TrackChoiceWindow"
    when: windowShown
    QtObject {
        id: backend
        property string columnResult: ""
        property var requests: []
        property int closes: 0
        property int total: 65000
        function requestColumn(field, search, cursor) {
            columnResult = ""
            requests = requests.concat([{field, search, cursor}])
        }
        function closeColumn() { closes++; columnResult = "" }
        function answer() {
            const request = requests[requests.length - 1]
            const start = request.cursor === "" ? 0 : Number(request.cursor)
            const end = Math.min(total, start + 100)
            const items = []
            for (let i = start; i < end; ++i) items.push({key: String(i), label: "Choice " + i, count: i + 1})
            columnResult = JSON.stringify({field: request.field, search: request.search, cursor: request.cursor,
                items, next: String(end), more: end < total, error: ""})
        }
    }
    TrackChoiceWindow { id: choices; model: backend }
    Component { id: windowMaker; TrackChoiceWindow { model: backend } }
    property bool restartOnArrival: false
    property var beforeState: ({})
    Connections {
        target: choices
        function onAboutToChange() {
            test.beforeState = {count: choices.count, first: choices.count ? choices.optionAt(0).key : ""}
        }
        function onPageArrived(items) {
            if (!test.restartOnArrival) return
            test.restartOnArrival = false
            choices.reset("album", "replacement")
        }
    }
    SignalSpy { id: shifts; target: choices; signalName: "shifted" }
    SignalSpy { id: changes; target: choices; signalName: "aboutToChange" }
    SignalSpy { id: arrivals; target: choices; signalName: "pageArrived" }
    function init() {
        failOnWarning(/ReferenceError|TypeError|Binding loop detected|ListModel/)
        choices.close()
        backend.requests = []
        backend.total = 65000
        restartOnArrival = false
        shifts.clear(); arrivals.clear(); changes.clear()
        beforeState = ({})
    }
    function cleanup() { choices.close() }
    function first() { choices.reset("title", ""); backend.answer() }
    function next() { choices.loadNext(); backend.answer() }
    function test_no_work_before_open_after_close_or_while_idle() {
        compare(backend.requests.length, 0)
        choices.loadNext(); choices.loadPrevious()
        compare(backend.requests.length, 0)
        first()
        compare(backend.requests.length, 1)
        compare(choices.count, 100)
        verify(!choices.loading)
        choices.close()
        compare(choices.count, 0)
        compare(choices.pages.length, 0)
        compare(choices.checkpoints.length, 0)
        backend.answer()
        choices.loadNext(); choices.loadPrevious()
        compare(backend.requests.length, 1)
        compare(choices.count, 0)
    }
    function test_three_pages_append_prepend_and_keep_anchor() {
        first()
        compare(beforeState.count, 0)
        next(); next()
        compare(choices.count, 300)
        compare(shifts.count, 0)
        const stableModel = choices.rows
        next()
        compare(beforeState.count, 300)
        compare(beforeState.first, "0")
        compare(choices.rows, stableModel)
        compare(choices.count, 300)
        compare(choices.optionAt(0).key, "100")
        compare(choices.optionAt(299).key, "399")
        compare(shifts.signalArguments[0][0], -100)
        verify(choices.previous)
        choices.loadPrevious(); backend.answer()
        compare(beforeState.first, "100")
        compare(choices.count, 300)
        compare(choices.optionAt(0).key, "0")
        compare(choices.optionAt(299).key, "299")
        compare(shifts.signalArguments[1][0], 100)
        verify(!choices.previous)
        compare(arrivals.count, 5)
        compare(changes.count, 5)
    }
    function test_last_partial_page_and_empty_result_are_reachable() {
        backend.total = 315
        first(); next(); next(); next()
        compare(choices.count, 215)
        compare(choices.optionAt(214).key, "314")
        verify(!choices.more)
        const sent = backend.requests.length
        choices.loadNext()
        compare(backend.requests.length, sent)
        backend.total = 0
        choices.reset("album", "missing"); backend.answer()
        compare(choices.count, 0)
        verify(!choices.more)
        verify(!choices.previous)
    }
    function test_stale_replies_busy_guards_and_errors() {
        choices.reset("title", "old")
        choices.reset("artist", "new")
        backend.columnResult = JSON.stringify({field:"title",search:"old",cursor:"",items:[],next:"",more:false})
        compare(choices.count, 0)
        verify(choices.loading)
        choices.loadNext(); choices.loadPrevious()
        compare(backend.requests.length, 2)
        backend.answer()
        choices.loadNext()
        backend.columnResult = JSON.stringify({field:"artist",search:"new",cursor:"",items:[],next:"",more:false})
        verify(choices.loading)
        backend.columnResult = JSON.stringify({field:"artist",search:"new",cursor:"100",error:"Unavailable"})
        verify(!choices.loading)
        compare(choices.error, "Unavailable")
        compare(choices.count, 100)
        choices.loadNext(); backend.answer()
        compare(choices.error, "")
        compare(choices.count, 200)
    }
    function test_destruction_cancels_and_reentrant_reset_keeps_new_request() {
        const other = createTemporaryObject(windowMaker, test)
        other.reset("title", "")
        const closed = backend.closes
        other.destroy()
        wait(1)
        compare(backend.closes, closed + 1)
        restartOnArrival = true
        first()
        compare(choices.field, "album")
        compare(choices.search, "replacement")
        compare(choices.count, 0)
        verify(choices.loading)
        backend.answer()
        compare(choices.count, 100)
        verify(!choices.loading)
    }
    function test_retry_preserves_the_previous_page_direction() {
        first(); next(); next(); next()
        choices.loadPrevious()
        backend.columnResult = JSON.stringify({field:"title",search:"",cursor:"",error:"Unavailable"})
        compare(choices.error, "Unavailable")
        verify(!choices.loading)
        compare(choices.optionAt(0).key, "100")
        choices.retry()
        compare(backend.requests[backend.requests.length - 1].cursor, "")
        backend.answer()
        compare(choices.optionAt(0).key, "0")
        compare(choices.error, "")
        compare(choices.failedJob, null)
    }
    function test_sparse_checkpoints_reconstruct_missing_previous_page() {
        first()
        for (let page = 1; page <= 520; ++page) next()
        verify(choices.stride > 1)
        verify(choices.checkpoints.length <= 512)
        compare(choices.count, 300)
        compare(choices.pages.length, 3)
        compare(choices.optionAt(0).key, "51800")
        const sent = backend.requests.length
        const changed = changes.count
        choices.loadPrevious()
        compare(backend.requests[sent].cursor, "51600")
        backend.columnResult = JSON.stringify({field:"title",search:"",cursor:"51600",error:"Unavailable"})
        compare(choices.failedJob.target, 517)
        choices.retry()
        compare(backend.requests[backend.requests.length - 1].cursor, "51600")
        backend.answer()
        verify(choices.loading)
        compare(changes.count, changed, "checkpoint traversal does not change the resident view")
        compare(choices.optionAt(0).key, "51800")
        compare(backend.requests[backend.requests.length - 1].cursor, "51700")
        backend.answer()
        verify(!choices.loading)
        compare(choices.optionAt(0).key, "51700")
        compare(choices.optionAt(299).key, "51999")
        compare(shifts.signalArguments[shifts.count - 1][0], 100)
        verify(choices.checkpoints.length <= 512)
        choices.loadPrevious(); backend.answer()
        choices.loadPrevious()
        backend.answer()
        verify(choices.loading, "reconstruction is between its checkpoint and target")
        const interrupted = backend.requests.length
        choices.close()
        compare(choices.failedJob, null)
        backend.answer()
        compare(choices.count, 0)
        compare(backend.requests.length, interrupted, "closing does not continue reconstruction")
    }
}
