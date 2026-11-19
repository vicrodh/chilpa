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
        Ratings.pendingTracks = ({})
        Prefs.animatedIndicator = false
        Motion.reduced = false
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
}
