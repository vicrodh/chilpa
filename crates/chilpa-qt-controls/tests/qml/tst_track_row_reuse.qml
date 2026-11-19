import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.offline
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

// A list that scrolls itself recycles its rows: a row that leaves waits in
// the pool and comes back as another track with the body it already had.
// Nothing of the track it was may stay on it, and while it waits it reads,
// draws and animates nothing.
TestCase {
    id: test
    name: "TrackRowReuse"
    when: windowShown
    visible: true
    width: 1100; height: 400

    TrackListModel { id: tracks }
    TrackList { id: list; width: 1000; height: 300; model: tracks; rich: false; showPlays: true }
    readonly property Item view: findChild(list, "trackListView")
    readonly property var covers: ["", "qrc:/qt/qml/app/chilpa/kit/assets/logo.png", "qrc:/qt/qml/app/chilpa/kit/assets/app-icon-dark.png"]
    // A cover no other picture in the suite shares: one green square.
    Canvas {
        id: green
        x: 1050; y: 350; width: 40; height: 40
        onPaint: { const context = getContext("2d"); context.fillStyle = "#00ff00"; context.fillRect(0, 0, width, height) }
    }

    // What the fixture's row at position i holds; every visible value names its row.
    function entry(i) {
        const featured = i % 4 === 2
        return { trackId: i + 1, rowKind: i % 50 === 25 ? 1 : 0, number: i + 1, title: "Track " + i, artist: "Band " + i,
            album: "Album " + i, sourceName: i % 2 ? "Source " + i : "", sourceInfo: "{}", composer: "",
            duration: Math.floor(i / 60) + ":" + String(i % 60).padStart(2, "0"), plays: 1000 + i, quality: "Q " + i, tier: i % 3,
            available: i % 7 !== 3, playing: i === 200, selected: i % 9 === 4, headerLabel: "Disc " + i, headerMeta: "", art: covers[i % 3],
            explicitContent: i % 4 === 1, aiGenerated: i % 5 === 2, group: "G" + Math.floor(i / 10),
            credits: featured ? JSON.stringify({ links: [{ name: "Guest " + i, target: "source", key: "guest" }, { name: "Band " + i, target: "source", key: "band" }],
                featured_html: "feat. <a href=\"0\">Guest " + i + "</a>", artist_html: "<a href=\"1\">Band " + i + "</a>" }) : "" }
    }
    function fill(count) {
        const stars = {}, hearts = {}, marks = {}
        for (let i = 0; i < count; ++i) {
            if (i % 5) stars[i + 1] = i % 5
            if (i % 6 === 0) hearts["track:" + (i + 1)] = true
            if (i % 8 === 0) marks[i + 1] = { state: "ready" }
        }
        Ratings.tracks = stars; Ratings.favorites = hearts; Offline.marks = marks
        Ratings.rev++; Offline.rev++
        tracks.clear()
        for (let i = 0; i < count; ++i) tracks.append(entry(i))
        view.forceLayout()
    }
    function init() {
        Prefs.ratings = true; Prefs.trackComposers = false; Prefs.animatedIndicator = false
        Player.playing = true; Player.paused = false
        // Row arrivals fade in; this suite is about scrolling, not inserts.
        Motion.reduced = true
        view.section.property = ""
        list.rich = false
        fill(400)
        view.positionViewAtBeginning()
        view.forceLayout()
    }
    function cleanup() {
        view.hovered = -1; view.actionHovered = 0
        Ratings.pendingTracks = ({}); Ratings.sources = ({})
        Prefs.animatedIndicator = false
        Motion.reduced = false
        list.height = 300
    }

    function slots() { return view.contentItem.children.filter(item => item.contentBuilds !== undefined && item.rowKind !== undefined) }
    function surfaces() { return view.contentItem.children.filter(item => item.rowIndex !== undefined && item.listMode !== undefined) }
    function body(row) { return findChild(row, "trackRowBody") }
    function findItem(item, wanted) {
        for (const child of item.children) {
            if (wanted(child)) return child
            const found = findItem(child, wanted)
            if (found) return found
        }
        return null
    }
    function shownTexts(item, into) {
        for (const child of item.children) {
            if (!child.visible) continue
            if (typeof child.text === "string" && child.text !== "") into.push(child.text)
            shownTexts(child, into)
        }
        return into
    }
    // A long scroll, a third of the screen a step, as a wheel or a flick
    // moves it, and then a turn of the event loop, as between two frames.
    function sweep(to) {
        const step = (to > view.contentY ? 1 : -1) * view.height / 3
        while (Math.abs(to - view.contentY) > Math.abs(step)) { view.contentY += step; view.forceLayout() }
        view.contentY = to; view.forceLayout()
        wait(0)
    }
    function lastY() { return view.originY + view.contentHeight - view.height }
    // That share of the way down, wherever the view's origin is.
    function at(share) { return view.originY + (lastY() - view.originY) * share }

    // Every cell shows the row it is now, and nothing names another one.
    function checkRow(row) {
        const i = row.index, data = entry(i)
        const shown = shownTexts(body(row), []).join("\n")
        const named = /\b(Track|Band|Album|Guest|Disc|Source|Q) (\d+)\b/g
        for (let match = named.exec(shown); match; match = named.exec(shown))
            compare(Number(match[2]), i, "row " + i + " shows " + match[0])
        if (data.rowKind !== 0) {
            verify(shown.includes("Disc " + i), "header " + i + " shows its label")
            return
        }
        compare(findChild(row, "creditedText").text, data.title)
        compare(findChild(row, "titleCredit").visible, data.credits !== "", "the credit of row " + i)
        compare(row.artistLabel.textFormat, data.credits !== "" ? Text.RichText : Text.PlainText, "the artist format of row " + i)
        verify(row.artistLabel.text.includes(data.artist), "row " + i + " artist: " + row.artistLabel.text)
        if (row.albumText.visible) compare(row.albumText.text, data.album)
        for (const cell of [data.duration, data.quality, String(data.plays)])
            verify(shown.includes(cell), "row " + i + " shows " + cell)
        compare(findChild(row, "trackNumber").text, String(data.number))
        compare(row.creditedTitle.explicitContent, data.explicitContent)
        compare(row.creditedTitle.aiGenerated, data.aiGenerated)
        compare(row.sourceBadge.name, data.sourceName)
        compare(row.tier, data.tier)
        compare(body(row).ratingStars.value, Ratings.tracks[i + 1] || 0, "the stars of row " + i)
        compare(row.end.favorite, !!Ratings.favorites["track:" + (i + 1)], "the heart of row " + i)
        compare(findChild(row, "trackOfflineMark").active, !!Offline.marks[i + 1], "the offline mark of row " + i)
        compare(row.end.visible, data.available)
        compare(row.opacity, data.available ? 1 : Metrics.disabledOpacity)
        if (list.rich) {
            const tile = findItem(body(row), item => item.fallbackGlyph !== undefined && item.edged !== undefined)
            verify(tile !== null, "row " + i + " has its cover tile")
            compare(String(tile.source), data.art)
            compare(tile.tint, i)
            const image = tile.children.find(child => child.retainWhileLoading !== undefined)
            const drawn = tile.children.some(child => child.visible && (child === image || child.fragmentShader !== undefined))
            if (drawn) compare(String(image.source), data.art, "row " + i + " draws only its own cover")
        }
    }
    // One surface under each shown row, at its place, in the state of the
    // row it is now: selected, playing, under the pointer or striped by its index.
    function checkSurfaces() {
        const shown = slots().filter(row => row.visible && !row.header)
        const lit = surfaces().filter(surface => surface.visible)
        compare(surfaces().length, slots().length, "a surface lives as long as its row")
        compare(lit.length, shown.length, "one surface for each shown row")
        for (const row of shown) {
            const own = lit.filter(surface => surface.y === row.y), data = entry(row.index)
            compare(own.length, 1, "row " + row.index + " has one surface")
            compare(own[0].rowIndex, row.index)
            const color = data.selected ? Theme.color.rowSelected : data.playing && list.rich ? Theme.color.rowPlaying
                : row.index === view.hovered ? Theme.color.stateHover : row.index % 2 === 1 ? Theme.color.stateStripe : "transparent"
            verify(Qt.colorEqual(own[0].color, color), "the surface of row " + row.index + ": " + own[0].color)
            compare(own[0].edge, data.playing && !Prefs.animatedIndicator, "the playing edge of row " + row.index)
        }
    }

    function test_scrolling_reuses_every_row_body_data() {
        return [{ tag: "compact", rich: false, height: Metrics.rowCompact }, { tag: "rich", rich: true, height: Metrics.rowRich }]
    }
    function test_scrolling_reuses_every_row_body(data) {
        list.rich = data.rich
        view.forceLayout()
        sweep(view.originY + view.height * 2); sweep(view.originY)
        wait(300)
        const bodies = new Map()
        for (const row of slots()) if (body(row)) bodies.set(row, body(row))
        const warm = slots().reduce((sum, row) => sum + row.contentBuilds, 0)
        let builds = 0, seenPlaying = false, seenSelected = false
        for (const to of [lastY(), view.originY, lastY() / 2, lastY(), view.originY]) {
            const before = new Map(slots().map(row => [row, [row.index, row.contentBuilds]]))
            sweep(to)
            for (const row of slots()) {
                const own = body(row)
                if (own && bodies.has(row)) compare(own, bodies.get(row), "row " + row.index + " keeps the body it had")
                else if (own) bodies.set(row, own)
                builds += row.contentBuilds - (before.has(row) ? before.get(row)[1] : 0)
            }
            const shown = slots().filter(row => row.visible && row.immediate)
            verify(shown.some(row => before.has(row) && before.get(row)[0] !== row.index), "the rows on screen came back from the pool")
            for (const row of slots()) verify(body(row) !== null, "row " + row.index + (row.visible ? "" : ", pooled,") + " has its body")
            for (const row of shown) checkRow(row)
            seenPlaying = seenPlaying || shown.some(row => row.playing)
            seenSelected = seenSelected || shown.some(row => row.selected)
            view.hovered = shown[shown.length >> 1].index
            checkSurfaces()
            view.hovered = -1
        }
        verify(seenPlaying && seenSelected, "the sweeps stopped on a playing and on a selected row")
        const screen = Math.ceil(view.height / data.height)
        verify(builds <= screen, "five long scrolls built " + builds + " bodies after the warm-up's " + warm)
        compare([...bodies.keys()].filter(row => slots().indexOf(row) < 0).length, 0, "no row was destroyed and made again")
        // The list held compact rows first, and its pool keeps the slots it had.
        const most = Math.ceil(view.height / Metrics.rowCompact) * 2 + 10
        verify(bodies.size <= most, bodies.size + " slots for at most " + most)
    }

    function test_a_reused_row_takes_its_new_group_divider() {
        view.section.property = "group"
        fill(400)
        for (const to of [lastY() / 3, lastY(), view.originY]) {
            sweep(to)
            for (const row of slots().filter(row => row.visible && row.immediate)) {
                compare(row.groupStart, row.index % 10 === 0, "row " + row.index + " starts its group")
                compare(findChild(row, "groupStart").visible, row.index % 10 === 0)
                checkRow(row)
            }
        }
    }

    function test_a_pooled_row_keeps_its_body_and_does_nothing() {
        Prefs.animatedIndicator = true
        tracks.setProperty(4, "playing", true)
        const row = view.itemAtIndex(4)
        verify(row !== null)
        tryVerify(() => Pulse.held > 0, 1000, "on the screen the playing row moves")
        const own = body(row)
        verify(own !== null)
        for (let step = 0; step < 40 && row.visible; ++step) { view.contentY += Metrics.rowCompact; view.forceLayout() }
        wait(0)
        verify(!row.visible && row.index === 4 && row.playing, "the playing row waits in the pool as it was")
        compare(body(row), own, "the pooled row keeps its body")
        compare(Pulse.held, 0, "a pooled row holds no clock")
        const mark = findItem(own, item => item.moving !== undefined && typeof item.toggled === "function")
        verify(mark !== null)
        compare(mark.running, false); compare(mark.moving, false)
        compare(row.inViewport, false)
        compare(row.quality, ""); compare(row.rating, 0); compare(row.favorite, false)
        compare(surfaces().filter(surface => surface.visible && surface.y === row.y).length, 0, "its surface is hidden with it")
        view.hovered = row.index; view.actionHovered = 2; view.moveHovered = 1
        compare(row.end.hovered, 0, "a pooled row answers no pointer")
        compare(row.moveHovered, 0)
        verify(TipState.target !== row.end, "and leaves no tooltip on itself")
        view.hovered = -1; view.actionHovered = 0; view.moveHovered = 0
        view.positionViewAtBeginning(); view.forceLayout()
        tryVerify(() => Pulse.held > 0, 1000, "back on the screen it moves again")
        tracks.setProperty(4, "playing", false)
    }

    function test_a_surface_goes_with_its_row() {
        sweep(lastY() / 2)
        checkSurfaces()
        fill(20)
        wait(50)
        checkSurfaces()
        for (const row of slots().filter(row => row.visible && row.immediate)) checkRow(row)
        fill(400)
        const before = surfaces()
        // A jump lets every row go without pooling it.
        list.jumpTo(300)
        view.forceLayout(); wait(300)
        compare(before.filter(surface => surfaces().indexOf(surface) >= 0).length, 0, "the rows let go took their surfaces")
        checkSurfaces()
        for (const row of slots().filter(row => row.visible && row.immediate)) checkRow(row)
        tracks.clear()
        view.forceLayout(); wait(50)
        compare(surfaces().length, slots().length, "no surface outlives its row")
        compare(surfaces().filter(surface => surface.visible).length, 0)
    }

    // A reader that writes down each key it is asked for.
    function counting(target, into) {
        return new Proxy(target, { get(object, key) { if (typeof key === "string") into.push(key); return object[key] } })
    }
    function tile(row) { return findItem(row, item => item.fallbackGlyph !== undefined && item.edged !== undefined) }
    // The color the window draws at the middle of an item, rendered now.
    function pixelAt(item) {
        const point = item.mapToItem(null, item.width / 2, item.height / 2)
        return grabImage(test).pixel(point.x, point.y)
    }
    // A scroll with a frame between its steps, as a listener's: the rows coming
    // in through the margin have the time to build their bodies.
    function stroll(to) {
        const step = (to > view.contentY ? 1 : -1) * view.height / 3
        while (Math.abs(to - view.contentY) > Math.abs(step)) { view.contentY += step; view.forceLayout(); wait(20) }
        view.contentY = to; view.forceLayout(); wait(20)
    }
    function warm() {
        stroll(at(0.25)); stroll(view.originY)
        tryVerify(() => slots().every(row => body(row) !== null), 2000, "every row the list holds has built its body")
    }

    function test_a_pooled_row_reads_nothing_when_revisions_change() {
        warm()
        list.rich = true
        view.forceLayout(); wait(0)
        const pooled = slots().filter(row => !row.visible && body(row) !== null)
        verify(pooled.length > 0, "the compact rows past the rich ones wait in the pool")
        const away = pooled.map(row => String(row.trackId))
        const reads = { pending: [], markable: [], stars: [], favorite: [], offline: [] }
        Ratings.pendingTracks = counting({}, reads.pending)
        Ratings.sources = counting({}, reads.markable)
        Ratings.tracks = counting(Ratings.tracks, reads.stars)
        Ratings.favorites = counting(Ratings.favorites, reads.favorite)
        Offline.marks = counting(Offline.marks, reads.offline)
        for (const kind in reads) reads[kind].length = 0
        Ratings.rev++; Offline.rev++
        for (const kind in reads)
            compare(reads[kind].filter(key => away.indexOf(key.replace("track:", "")) >= 0), [], "a pooled row reads no " + kind)
        verify(reads.pending.length > 0 && reads.markable.length > 0 && reads.offline.length > 0, "the rows on the screen read theirs")
        // While its row waited, the track changed; the first frame it is back on says so.
        const back = pooled.filter(row => row.index < Math.floor(view.height / Metrics.rowCompact))[0]
        verify(back !== undefined, "a pooled row that compact rows show again")
        const id = back.trackId
        Ratings.pendingTracks[id] = true
        Offline.marks[id] = { state: "downloading", fraction: 0.5 }
        Ratings.rev++; Offline.rev++
        list.rich = false
        view.forceLayout()
        const shown = slots().find(row => row.visible && row.immediate && row.trackId === id)
        verify(shown !== undefined, "track " + id + " is on the screen again")
        compare(shown.end.busy, true, "its heart waits for the change")
        const mark = findChild(shown, "trackOfflineMark")
        verify(mark.active && mark.item.phase === "downloading", "its offline state is the new one")
        compare(mark.item.fraction, 0.5)
    }

    function test_a_reused_row_never_draws_the_cover_it_had() {
        waitForRendering(green)
        const cover = green.toDataURL("image/png")
        // The first screens show one cover; every row past them has a cover of its own, never asked for before.
        const run = Date.now()
        for (let i = 0; i < 400; ++i)
            tracks.setProperty(i, "art", i < 40 ? cover : "qrc:/qt/qml/app/chilpa/kit/assets/app-icon-dark.png?row=" + i + "&run=" + run)
        list.rich = true
        view.forceLayout()
        warm()
        tryVerify(() => slots().filter(row => row.visible).every(row => tile(row) && tile(row).ready), 2000, "the first covers are drawn")
        const ownCover = pixelAt(tile(view.itemAtIndex(1)))
        verify(Qt.colorEqual(ownCover, "#00ff00"), "the first rows draw the green cover: " + ownCover)
        // Rows that go to the pool let their covers go.
        list.height = 150
        view.forceLayout()
        const pooled = slots().filter(row => !row.visible && tile(row) !== null)
        verify(pooled.length > 0, "rows with covers wait in the pool")
        for (const row of pooled) compare(String(tile(row).source), "", "pooled row " + row.index + " holds no cover")
        list.height = 300
        view.forceLayout()
        warm()
        // A row that comes back from the pool as a track of the same cover has it at once.
        const was = new Map(slots().map(row => [row, row.index]))
        view.contentY += Metrics.rowRich * 3; view.forceLayout()
        const again = slots().filter(row => row.visible && row.index < 40 && was.get(row) !== row.index && body(row) !== null)
        verify(again.length > 0, "rows came back from the pool with their bodies")
        for (const row of again) verify(tile(row).ready, "row " + row.index + " has the cover its neighbours hold")
        // Rows that come back as tracks whose covers are still on their way draw the placeholder, not the green:
        // the ones with a body on this very frame, and every one on the next.
        view.contentY = view.originY + Metrics.rowRich * 50; view.forceLayout()
        const now = slots().filter(row => row.visible && row.immediate && body(row) !== null)
        verify(now.every(row => row.index >= 40), "the screen shows only rows past the green ones")
        verify(now.length > 0, "rows came back with their bodies, as tracks with other covers")
        for (const row of now) {
            verify(String(tile(row).source).includes("row=" + row.index + "&"), "row " + row.index + " asks for its own cover")
            verify(!Qt.colorEqual(pixelAt(tile(row)), "#00ff00"), "row " + row.index + " does not draw the cover its slot had")
        }
        wait(0)
        const arriving = slots().filter(row => row.visible && row.immediate)
        for (const row of arriving) verify(!Qt.colorEqual(pixelAt(tile(row)), "#00ff00"), "row " + row.index + " on the first frame")
        tryVerify(() => arriving.every(row => tile(row).ready), 2000, "their own covers arrive")
        for (const row of arriving) verify(!Qt.colorEqual(pixelAt(tile(row)), "#00ff00"))
    }
}
