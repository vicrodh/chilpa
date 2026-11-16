import QtQuick
import app.chilpa.kit

// Three resident pages and at most 512 cursor checkpoints belong to one open
// checklist. Older cursors become sparse; scrolling back rebuilds only the
// requested page from its closest checkpoint. No catalog grows with scrolling.
QtObject {
    id: window
    property var model: null
    property string field: ""
    property string search: ""
    property bool active: false
    property bool loading: false
    property string error: ""
    readonly property ListModel rows: ListModel { id: choices }
    readonly property int count: choices.count
    readonly property bool more: pages.length > 0 && pages[pages.length - 1].more
    readonly property bool previous: pages.length > 0 && pages[0].index > 0
    signal aboutToChange()
    signal shifted(int rows)
    signal pageArrived(var items)

    // Metadata has the same lifetime as rows; checkpoint cursors never retain
    // option arrays. One request, including backward reconstruction, is active.
    property var pages: []
    property var checkpoints: []
    property int stride: 1
    property int generation: 0
    property var pending: null
    property var failedJob: null
    readonly property Connections response: Connections {
        target: window.model
        function onColumnResultChanged() { window.accept(window.model.columnResult) }
    }

    function optionAt(index) { return index >= 0 && index < count ? choices.get(index) : null }
    function close() {
        generation++
        active = false
        pending = null
        failedJob = null
        loading = false
        if (model) model.closeColumn()
        choices.clear()
        pages = []
        checkpoints = []
        stride = 1
        error = ""
    }
    function reset(column, text) {
        close()
        field = column
        search = String(text || "").trim()
        active = true
        checkpoints = [{index: 0, cursor: ""}]
        request(0, "", 0, false)
    }
    function request(index, cursor, target, prepend) {
        if (!active || !model) return
        pending = {index, cursor, target, prepend, generation}
        failedJob = null
        loading = true
        error = ""
        model.requestColumn(field, search, cursor)
    }
    function loadNext() {
        if (!active || loading) return
        if (pages.length === 0) { request(0, "", 0, false); return }
        const last = pages[pages.length - 1]
        if (last.more) request(last.index + 1, last.next, last.index + 1, false)
    }
    function loadPrevious() {
        if (!active || loading || !previous) return
        const target = pages[0].index - 1
        let start = checkpoints[0]
        for (const point of checkpoints) {
            if (point.index > target) break
            start = point
        }
        request(start.index, start.cursor, target, true)
    }
    function retry() {
        if (!active || loading || !failedJob) return
        const job = failedJob
        request(job.index, job.cursor, job.target, job.prepend)
    }
    function remember(index, cursor) {
        if (index % stride !== 0) return
        let points = checkpoints.slice()
        const at = points.findIndex(point => point.index >= index)
        if (at >= 0 && points[at].index === index) points[at] = {index, cursor}
        else points.splice(at < 0 ? points.length : at, 0, {index, cursor})
        if (points.length > 512) {
            stride *= 2
            points = points.filter(point => point.index % stride === 0)
        }
        checkpoints = points
    }
    function fail(message) {
        // Retain only the failed cursor and direction, never an option page.
        failedJob = pending
        pending = null
        error = message || I18n.tr("The library could not be read.", I18n.rev)
        loading = false
    }
    function accept(encoded) {
        if (!active || !pending || !encoded) return
        let result
        try { result = JSON.parse(encoded) } catch (_) { fail(""); return }
        const job = pending
        if (result.field !== field || result.search !== search || result.cursor !== job.cursor) return
        if (result.error) { fail(result.error); return }
        if (!Array.isArray(result.items) || result.items.length > 100
            || typeof result.next !== "string" || (result.more && (!result.items.length || result.next === job.cursor))) { fail(""); return }
        remember(job.index, job.cursor)
        if (result.more) remember(job.index + 1, result.next)
        if (job.index < job.target) {
            if (!result.more) { fail(""); return }
            request(job.index + 1, result.next, job.target, job.prepend)
            return
        }
        aboutToChange()
        if (!active || generation !== job.generation) return
        let resident = pages.slice(), shift = 0
        // Remove before inserting, so even during model notifications there
        // are never more than 300 resident options. Their roles remain stable.
        if (resident.length === 3) {
            if (job.prepend) {
                const removed = resident.pop()
                choices.remove(choices.count - removed.count, removed.count)
            } else {
                const removed = resident.shift()
                choices.remove(0, removed.count)
                shift -= removed.count
            }
        }
        const page = {index: job.index, cursor: job.cursor, next: result.next, more: !!result.more, count: result.items.length}
        if (job.prepend) {
            for (let i = result.items.length - 1; i >= 0; --i) choices.insert(0, result.items[i])
            resident.unshift(page)
            shift += result.items.length
        } else {
            for (const item of result.items) choices.append(item)
            resident.push(page)
        }
        pages = resident
        pending = null
        if (shift) shifted(shift)
        pageArrived(result.items)
        if (active && generation === job.generation) loading = false
    }
    Component.onDestruction: if (active && model) model.closeColumn()
}
