import QtQuick
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.offline
import app.chilpa.testing
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

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
    property real flickVelocity: 0
    function initTestCase() { flickVelocity = view.maximumFlickVelocity }
    readonly property var covers: ["", "qrc:/qt/qml/app/chilpa/kit/assets/logo.png", "qrc:/qt/qml/app/chilpa/kit/assets/app-icon-dark.png"]
    // A cover no other picture in the suite shares: one green square.
    Canvas {
        id: green
        x: 1050; y: 350; width: 40; height: 40
        onPaint: { const context = getContext("2d"); context.fillStyle = "#00ff00"; context.fillRect(0, 0, width, height) }
    }
    // Every body the list builds, also those it throws away at once: one surface each.
    Connections {
        id: births
        target: test.view ? test.view.contentItem : null
        property var seen: new Set()
        function onChildrenChanged() { for (const child of test.view.contentItem.children) if (child.rowIndex !== undefined && child.listMode !== undefined) seen.add(child) }
    }
    // Every frame the window makes, as it takes the scene: on the offscreen platform the software render loop emits
    // beforeSynchronizing on this thread, after the views are laid out and before the scene is synced and drawn.
    Connections {
        id: frames
        target: test.Window.window
        property int made: 0
        property int emptiest: 0
        function onBeforeSynchronizing() { made++; emptiest = Math.max(emptiest, test.emptyRows()) }
    }
    // What schedules a look at the pool of the list (TrackListViewport.band), and the view's content height.
    Connections { id: bands; target: null; property int changes: 0; function onBandChanged() { changes++ } }
    Connections { id: heights; target: test.view; property int changes: 0; function onContentHeightChanged() { changes++ } }
    // The Tracks page: a sheet that scrolls sideways, with the snapshot rows and the optional columns.
    Component {
        id: sheetPage
        Flickable {
            id: page
            readonly property alias list: sheetList
            readonly property alias model: sheetTracks
            width: 640; height: 300
            contentWidth: Math.max(width, sheetList.columns.minimumWidth)
            contentHeight: height
            clip: true
            TrackListModel { id: sheetTracks; kind: "tracks" }
            TrackList {
                id: sheetList
                width: page.contentWidth; height: page.height
                header: false; sheet: true; showPlays: true; rich: false
                visibleLeft: page.contentX; visibleRight: page.contentX + page.width
                optionalColumns: ["index", "art", "source", "quality", "bits", "rate", "format", "rating", "favorite", "plays", "time"]
                model: sheetTracks
            }
        }
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
        mouseMove(test, test.width - 2, test.height - 2)
        view.hovered = -1; view.actionHovered = 0
        Ratings.pendingTracks = ({}); Ratings.sources = ({})
        Prefs.animatedIndicator = false
        Motion.reduced = false
        view.cancelFlick()
        view.maximumFlickVelocity = test.flickVelocity
        list.visible = true
        list.height = 300
        list.cursorRow = -1
        bands.target = null
        test.forceActiveFocus()
        // Every held cover arrives, so that nothing waits for the test once it is over.
        HeldImages.open = "*"; HeldImages.open = ""
    }

    function slots() { return view.contentItem.children.filter(item => item.contentBuilds !== undefined && item.rowKind !== undefined) }
    // The rows the screen shows, by their place in the view, that have no body to draw.
    function emptyRows() { return slots().filter(row => row.visible && row.y + row.height > view.contentY && row.y < view.contentY + view.height && body(row) === null).length }
    function pool() { return list.resources.find(object => typeof object.trim === "function" && object.band !== undefined) }
    function budget() { return 3 * Math.ceil(view.height / Metrics.rowCompact) }
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
        // A row in the margin may still be building its first body; a row on the screen has its own.
        for (const row of slots().filter(row => row.visible && row.immediate)) verify(body(row) !== null, "row " + row.index + " on the screen has its body")
        const shown = slots().filter(row => row.visible && !row.header && body(row) !== null)
        const lit = surfaces().filter(surface => surface.visible)
        compare(surfaces().length, slots().filter(row => body(row) !== null).length, "a surface lives as long as its row's body")
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
        for (const to of [lastY(), view.originY, at(0.5), lastY(), view.originY]) {
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
        for (const to of [at(1 / 3), lastY(), view.originY]) {
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
        sweep(at(0.5))
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
        compare(surfaces().length, slots().filter(row => body(row) !== null).length, "no surface outlives its row's body")
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
    function built() { return births.seen.size }
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
    // What the row says is the row of the model it is now, after the model moved.
    function checkData(row) {
        const data = tracks.get(row.index)
        compare(findChild(row, "creditedText").text, data.title, "the title of row " + row.index)
        compare(findChild(row, "trackNumber").text, String(data.number))
        compare(row.sourceBadge.name, data.sourceName)
        verify(row.artistLabel.text.includes(data.artist), "row " + row.index + " artist: " + row.artistLabel.text)
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

    function picture(row) { return tile(row).children.find(child => child.retainWhileLoading !== undefined) }
    function pixelIn(shot, item) {
        const point = item.mapToItem(null, item.width / 2, item.height / 2)
        return shot.pixel(point.x, point.y)
    }
    // The green and the blue covers, also drawn through the dimmed opacity of a row that cannot play.
    function greenish(color) { return color.g > 0.9 && color.r < 0.7 && color.b < 0.7 }
    function bluish(color) { return color.b > 0.9 && color.r < 0.7 && color.g < 0.7 }
    function test_a_reused_row_never_draws_the_cover_it_had() {
        waitForRendering(green)
        const cover = green.toDataURL("image/png")
        // The first screens show one cover; every row past them has a blue cover of its own, held on its way until the test lets it come.
        for (let i = 0; i < 400; ++i)
            tracks.setProperty(i, "art", i < 40 ? cover : "image://held/row" + i + "/#0000ff")
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
        // Rows that come back as tracks whose covers are still on their way draw the placeholder, not the green,
        // for as long as their covers take: here, until the test lets them come.
        view.contentY = view.originY + Metrics.rowRich * 50; view.forceLayout()
        const now = slots().filter(row => row.visible && row.immediate)
        verify(now.every(row => row.index >= 40), "the screen shows only rows past the green ones")
        verify(now.length > 0 && now.every(row => body(row) !== null), "rows came back with their bodies, as tracks with other covers")
        for (const row of now) {
            compare(String(tile(row).source), "image://held/row" + row.index + "/#0000ff", "row " + row.index + " asks for its own cover")
            compare(picture(row).status, Image.Loading, "the cover of row " + row.index + " is on its way")
        }
        for (const pause of [0, 100]) {
            wait(pause)
            const shot = grabImage(test)
            for (const row of now) {
                compare(picture(row).status, Image.Loading, "the cover of row " + row.index + " is still on its way")
                const drawn = pixelIn(shot, tile(row))
                verify(!greenish(drawn) && !bluish(drawn), "row " + row.index + " draws its placeholder: " + drawn)
            }
        }
        for (const row of now) { HeldImages.open = "row" + row.index; HeldImages.open = "" }
        tryVerify(() => now.every(row => tile(row).ready), 2000, "their own covers arrive")
        const shot = grabImage(test)
        for (const row of now) verify(bluish(pixelIn(shot, tile(row))), "row " + row.index + " draws its own cover: " + pixelIn(shot, tile(row)))
    }

    // Where the source tooltip is now: on the badge under the pointer, saying that row's source, or nowhere.
    function checkTip(x, y, what) {
        const row = view.itemAtIndex(view.indexAt(x, y + view.contentY))
        if (TipState.target === null) return
        verify(TipState.target.visible, what + ": the tooltip's badge is shown")
        verify(row !== null && TipState.target === row.sourceBadge, what + ": the tooltip is on the badge under the pointer")
        compare(TipState.text, row.sourceName, what + ": it says the source of row " + row.index)
        compare(TipState.key, row.trackId, what + ": it stands for track " + row.trackId)
    }
    function test_a_source_tooltip_follows_its_track() {
        warm()
        const row = view.itemAtIndex(9)
        const x = list.columns.sourceX + list.columns.sourceWidth / 2, y = row.y - view.contentY + row.height / 2
        mouseMove(view, x, y)
        tryVerify(() => TipState.shown && TipState.target === row.sourceBadge, 2000, "the badge says its source")
        compare(TipState.text, "Source 9")
        // Rich rows are taller: under a still pointer, row 9 goes to the pool. The list asks again once the rows are laid out, a turn later.
        list.rich = true
        waitForRendering(view); wait(0)
        compare(row.visible, false, "the row of the tooltip waits in the pool")
        checkTip(x, y, "rich")
        verify(TipState.target !== row.sourceBadge, "the pooled badge keeps no tooltip")
        list.rich = false
        waitForRendering(view); wait(0)
        checkTip(x, y, "compact again")
        // A reset under a still pointer: the same row object stays there, now another track.
        const under = view.itemAtIndex(view.indexAt(x, y + view.contentY))
        compare(under.index, 9)
        mouseMove(view, x, y + 1)
        tryVerify(() => TipState.shown && TipState.target === under.sourceBadge, 2000, "the badge under the pointer says its source")
        for (let i = 0; i < 400; ++i) tracks.set(i, entry((i + 2) % 400))
        tracks.modelReset()
        wait(0)
        compare(view.itemAtIndex(view.indexAt(x, y + view.contentY)), under, "the same row object is under the pointer")
        compare(under.trackId, 12, "now showing another track")
        checkTip(x, y, "after the reset")
        tryVerify(() => TipState.shown && TipState.target === under.sourceBadge && TipState.text === "Source 11", 2000, "it says the new track's source: " + TipState.text)
    }

    // The row under a still pointer changes in place, as the bridge's dataChanged does it: no reset, no move, no new height.
    function test_a_source_tooltip_follows_a_change_in_place() {
        warm()
        const row = view.itemAtIndex(9)
        const x = list.columns.sourceX + list.columns.sourceWidth / 2, y = row.y - view.contentY + row.height / 2
        mouseMove(view, x, y)
        tryVerify(() => TipState.shown && TipState.target === row.sourceBadge, 2000, "the badge says its source")
        compare(TipState.text, "Source 9")
        const resets = list.rowsRev, height = view.contentHeight, top = view.contentY
        // The same track, now from another source.
        tracks.setProperty(9, "sourceName", "Source moved")
        tryCompare(TipState, "text", "Source moved", 1000, "the tooltip says the new source")
        verify(TipState.shown && TipState.target === row.sourceBadge && TipState.key === 10, "of the same track, without asking anew")
        // Another track in the same row.
        tracks.set(9, entry(11))
        tryVerify(() => TipState.key === 12 && TipState.text === "Source 11", 1000, "it stands for the track now in the row: " + TipState.key + ", " + TipState.text)
        compare(TipState.target, row.sourceBadge)
        tryVerify(() => TipState.shown, 2000, "and says it after its delay")
        // The track cannot play for a while: its menu button goes with its end, and comes back with it.
        mouseMove(view, list.columns.moreX + row.end.favoriteWidth + Metrics.rowAction / 2, y)
        tryVerify(() => TipState.shown && TipState.target === row.end, 2000, "the menu button says what it does")
        for (const pause of [0, 50]) {
            tracks.setProperty(9, "available", false)
            compare(TipState.target, null, "a hidden end says nothing")
            wait(pause)
            tracks.setProperty(9, "available", true)
            tryVerify(() => TipState.target === row.end, 1000, "shown again under the still pointer " + pause + " ms later, it says it again")
        }
        compare(view.itemAtIndex(9), row, "the same row object all along")
        compare(list.rowsRev, resets, "no reset"); compare(view.contentHeight, height, "no new height"); compare(view.contentY, top, "no scroll")
        tracks.set(9, entry(9))
    }

    // The menu button under a still pointer says the offline state of the track it stands for: as a download
    // goes, finishes or fails and its copy is removed, and when the row becomes another track in place.
    function test_the_menu_button_tooltip_follows_the_offline_state_and_the_track() {
        warm()
        const row = view.itemAtIndex(9)
        mouseMove(view, list.columns.moreX + row.end.favoriteWidth + Metrics.rowAction / 2, row.y - view.contentY + row.height / 2)
        tryVerify(() => TipState.shown && TipState.target === row.end, 2000, "the menu button says what it does")
        compare(TipState.text, "More")
        for (const [mark, said] of [[{ state: "downloading", fraction: 0.5 }, "\nDownloading for offline listening"], [{ state: "ready" }, "\nAvailable offline"],
                                     [{ state: "failed", error: "No room left" }, "\nNo room left"], [undefined, ""]]) {
            Offline.marks[10] = mark
            Offline.rev++
            compare(TipState.text, "More" + said, "it says the offline state now: " + JSON.stringify(mark))
            verify(TipState.shown && TipState.target === row.end, "without asking anew")
        }
        // Another track in the same row, kept offline.
        tracks.set(9, entry(16))
        compare(view.itemAtIndex(9), row, "the same row object")
        compare(row.trackId, 17)
        tryCompare(TipState, "text", "More\nAvailable offline", 1000, "it says the offline state of the track now in the row")
        compare(TipState.target, row.end)
        tracks.set(9, entry(9))
    }

    function test_the_pool_holds_at_most_a_band_of_bodies() {
        list.height = 676
        view.forceLayout(); wait(0)
        warm()
        const held = slots().length
        list.height = 176
        wait(0)
        const budget = 3 * Math.ceil(view.height / Metrics.rowCompact)
        const pooled = slots().filter(row => !row.visible).length
        verify(pooled <= budget, pooled + " rows in the pool for a budget of " + budget + ", after holding " + held)
        // Within its budget the pool keeps its bodies: rich rows and back to compact build none.
        list.height = 300
        view.forceLayout(); wait(0)
        warm()
        const before = built()
        list.rich = true; view.forceLayout(); wait(0)
        list.rich = false; view.forceLayout(); wait(0)
        compare(built() - before, 0, "rich and back to compact built no body")
        // A warm pool scrolls at any speed without building: two screens a frame, with a turn of the event loop
        // after each step, where a look at the pool would run, and counted after the last one.
        const fast = built(), rows = slots()
        for (const to of [lastY(), view.originY]) {
            const step = (to > view.contentY ? 2 : -2) * view.height
            while (Math.abs(to - view.contentY) > Math.abs(step)) { view.contentY += step; view.forceLayout(); wait(0) }
            view.contentY = to; view.forceLayout(); wait(0)
        }
        wait(0)
        compare(built() - fast, 0, "two screens a frame built no body")
        compare(rows.filter(row => slots().indexOf(row) < 0).length, 0, "and let no row go")
        // A list a search empties keeps its pool, and filled again builds nothing.
        const kept = slots().length, again = built()
        verify(kept <= 3 * Math.ceil(view.height / Metrics.rowCompact), kept + " rows held")
        tracks.clear()
        wait(0)
        compare(slots().length, kept, "the emptied list keeps the rows it pooled")
        fill(400)
        view.positionViewAtBeginning(); view.forceLayout(); wait(0)
        compare(built() - again, 0, "filled again, it built no body")
    }

    // Hidden, every row is hidden with its parent, but only the list's pool is the pool: a list shrunk while hidden
    // lets its pool go, and shown again keeps at most its budget.
    function test_a_list_shrunk_while_hidden_lets_its_pool_go() {
        list.height = 676
        view.forceLayout(); wait(0)
        warm()
        const rows = slots()
        list.visible = false
        list.height = 176
        wait(0)
        verify(rows.some(row => slots().indexOf(row) < 0), "hidden, the list let its pool go: " + slots().length + " rows held of " + rows.length)
        list.visible = true
        wait(0)
        const pooled = slots().filter(row => !row.visible).length
        verify(pooled <= budget(), pooled + " rows in the pool for a budget of " + budget())
        for (const row of slots().filter(row => row.visible && row.immediate)) checkRow(row)
    }

    // A list that shrinks while it moves lets its pool go when it stops, not mid-flick. Qt itself releases a few
    // rows without pooling them when the view shrinks during a flick; the list's own drain is turning reuse off and on.
    Connections { id: drains; target: test.view; property int toggles: 0; function onReuseItemsChanged() { toggles++ } }
    function test_a_list_that_shrinks_while_it_moves_lets_its_pool_go_when_it_stops() {
        list.height = 676
        view.forceLayout(); wait(0)
        warm()
        view.flick(0, -view.maximumFlickVelocity)
        tryVerify(() => view.moving, 1000, "the list moves")
        drains.toggles = 0
        list.height = 176
        for (let turn = 0; turn < 5; ++turn) wait(16)
        verify(view.moving, "still moving")
        compare(drains.toggles, 0, "the list did not drain its pool while it moved")
        verify(slots().filter(row => !row.visible).length > budget(), "the pool is over its budget meanwhile")
        tryVerify(() => !view.moving, 5000, "the flick ends")
        wait(0)
        compare(drains.toggles, 2, "stopped, it drained its pool once")
        verify(slots().filter(row => !row.visible).length <= budget(), slots().filter(row => !row.visible).length + " rows in the pool")
    }

    // A look at the pool scheduled in the turn its list goes away finds no list, and does nothing.
    Component {
        id: bareList
        Item {
            property alias rows: rows
            property alias looks: looks
            ListView { id: rows; width: 100; height: 100; model: 50; reuseItems: true; delegate: Item { width: 100; height: 10 } }
            TrackListViewport { id: looks; model: null; view: rows }
        }
    }
    function test_a_look_at_the_pool_of_a_list_gone_in_its_turn_does_nothing() {
        failOnWarning(/TypeError/)
        const made = createTemporaryObject(bareList, test)
        made.rows.height = 50
        made.looks.view = null
        wait(0)
    }

    // A list that shrinks while its scrollbar is held lets its pool go when the scrollbar is let go, not during the drag.
    function test_a_list_that_shrinks_while_its_scrollbar_is_held_lets_its_pool_go_when_let_go() {
        list.height = 676
        view.forceLayout(); wait(0)
        warm()
        const bar = findChild(list, "trackListScrollbar")
        mouseMove(view, view.width / 2, 50)
        tryVerify(() => bar.visible, 1000, "the scrollbar shows")
        const at = bar.mapToItem(test, bar.width / 2, bar.height * (bar.position + bar.size / 2))
        mouseMove(test, at.x, at.y)
        mousePress(test, at.x, at.y)
        tryVerify(() => bar.pressed, 1000, "the scrollbar is held")
        drains.toggles = 0
        list.height = 176
        wait(0); wait(0)
        compare(drains.toggles, 0, "the list did not drain its pool while its scrollbar was held")
        verify(slots().filter(row => !row.visible).length > budget(), "the pool is over its budget meanwhile")
        mouseRelease(test, at.x, at.y)
        tryVerify(() => !bar.pressed, 1000, "the scrollbar is let go")
        wait(0)
        compare(drains.toggles, 2, "let go, it drained its pool once")
        verify(slots().filter(row => !row.visible).length <= budget(), slots().filter(row => !row.visible).length + " rows in the pool")
    }

    // A scrollbar dragged a screen and a half a step, with a frame between steps: every step lands past the rows the list
    // holds, so the view lets them all go to its pool and takes the screen back from it, the oldest first. The rows on the
    // screen show their own tracks before any frame, nothing is built, destroyed or made, and the pool keeps to its budget.
    // Each step logs what it cost: rows bound again, covers asked, rating and offline reads.
    function test_a_scrollbar_drag_binds_the_screen_again_and_builds_nothing_data() {
        return [{ tag: "compact" }, { tag: "rich", rich: true }, { tag: "grouped", grouped: true }]
    }
    function test_a_scrollbar_drag_binds_the_screen_again_and_builds_nothing(data) {
        list.rich = !!data.rich
        if (data.grouped) view.section.property = "group"
        fill(1000)
        compare(view.cacheBuffer === 0, !!data.grouped, "grouped, the list keeps no margin")
        view.positionViewAtBeginning(); view.forceLayout()
        warm()
        const bar = findChild(list, "trackListScrollbar")
        mouseMove(view, view.width / 2, 50)
        tryVerify(() => bar.visible, 1000, "the scrollbar shows")
        const grip = bar.mapToItem(test, bar.width / 2, bar.height * (bar.position + bar.size / 2))
        mouseMove(test, grip.x, grip.y)
        mousePress(test, grip.x, grip.y)
        tryVerify(() => bar.pressed, 1000, "the scrollbar is held")
        const px = Math.ceil(1.5 * view.height * bar.height * (1 - bar.size) / (view.contentHeight - view.height))
        const reads = [], kept = [Ratings.tracks, Ratings.favorites, Offline.marks]
        Ratings.tracks = counting(kept[0], reads); Ratings.favorites = counting(kept[1], reads); Offline.marks = counting(kept[2], reads)
        Ratings.pendingTracks = counting({}, reads); Ratings.sources = counting({}, reads)
        const held = slots(), bodies = surfaces(), log = [], screen = held.filter(row => row.visible && row.immediate).length
        let y = grip.y
        for (let step = 0; step < 12 && view.contentY < lastY() - view.height * 2; ++step) {
            const before = built(), top = view.contentY, was = new Map(held.map(row => [row, [row.index, String(row.art)]]))
            reads.length = 0; frames.made = 0; frames.emptiest = 0
            y += px
            mouseMove(test, grip.x, y, 0, Qt.LeftButton)
            const read = reads.length, empty = emptyRows()
            verify(view.contentY - top > view.height, "step " + step + " went " + (view.contentY - top) + " px, more than a screen")
            compare(empty, 0, "step " + step + ": every row on the screen has its body before any frame")
            for (const row of slots().filter(row => row.visible && row.immediate)) checkRow(row)
            wait(16)
            compare(frames.emptiest, 0, "step " + step + ": no frame showed a row on the screen without its body")
            compare(built() - before, 0, "step " + step + " built no body")
            compare(slots().filter(row => held.indexOf(row) < 0).length + held.filter(row => slots().indexOf(row) < 0).length, 0, "step " + step + " made and destroyed no row")
            compare(bodies.filter(surface => surfaces().indexOf(surface) < 0).length, 0, "step " + step + " threw no body away")
            const rebound = slots().filter(row => row.visible && was.get(row)[0] !== row.index)
            const asked = slots().filter(row => String(row.art) !== "" && String(row.art) !== was.get(row)[1])
            const pooled = slots().filter(row => !row.visible).length
            verify(pooled <= budget(), "step " + step + ": " + pooled + " rows in the pool for a budget of " + budget())
            log.push(Math.round(view.contentY - top) + " px: " + rebound.length + " rows bound again (" + rebound.filter(row => row.immediate).length + " on the screen), "
                + asked.length + " covers asked, " + read + " reads, " + pooled + " pooled")
        }
        mouseRelease(test, grip.x, y)
        tryVerify(() => !bar.pressed, 1000, "the scrollbar is let go")
        Ratings.tracks = kept[0]; Ratings.favorites = kept[1]; Offline.marks = kept[2]
        verify(log.length >= 8, log.length + " steps")
        console.log(data.tag + " scrollbar drag, " + px + " handle px a step, " + screen + " rows on the screen: " + log.join("; "))
    }

    // A scroll under a still pointer resolves it once a step: at once, or a turn later when the step's layout changed the
    // estimated height and asked for a look then. Counted by the turn-later looks, with the pointer's row checked each time.
    Connections { id: later; target: null; property int runs: 0; function onTriggered() { runs++ } }
    function test_a_scroll_under_a_still_pointer_resolves_it_once_a_step_data() {
        return [{ tag: "ungrouped" }, { tag: "grouped", grouped: true }]
    }
    function test_a_scroll_under_a_still_pointer_resolves_it_once_a_step(data) {
        if (data.grouped) { view.section.property = "group"; fill(400); view.positionViewAtBeginning(); view.forceLayout() }
        warm()
        const pointer = list.children.find(child => typeof child.track === "function" && child.rows === view)
        later.target = pointer.resources.find(object => object.interval === 0 && object.triggeredOnStart !== undefined)
        verify(later.target !== null, "the pointer's turn-later look")
        const y = 100
        mouseMove(view, view.width / 2, y)
        wait(0)
        let grown = 0
        for (let step = 0; step < 60; ++step) {
            const was = view.hovered, height = heights.changes
            later.runs = 0
            view.contentY += Metrics.rowCompact * 1.5
            const now = view.indexAt(Metrics.gap2, y + view.contentY), grew = heights.changes > height
            if (!grew) compare(view.hovered, now, "step " + step + ": the pointer is resolved at once")
            else if (now !== was) compare(view.hovered, was, "step " + step + ": its layout asked for a look a turn later, and that one is the look")
            wait(0)
            compare(view.hovered, now, "step " + step + ": the row under the pointer")
            compare(later.runs, grew ? 1 : 0, "step " + step + ": resolved once")
            if (grew) grown++
        }
        verify(grown > 0 && grown < 60, grown + " of 60 steps changed the estimated height")
        later.target = null
    }

    // What schedules a look at the pool, and what one costs: a scroll through groups, whose estimated content height
    // moves with every step, a flick, and a model that grows a row at a time, as a scan adds them.
    function test_scrolling_and_growing_leave_the_pool_alone() {
        const looks = pool()
        verify(looks !== undefined, "the list's pool")
        view.section.property = "group"
        fill(400)
        view.positionViewAtBeginning(); view.forceLayout()
        warm()
        bands.target = looks
        bands.changes = 0; heights.changes = 0
        for (const to of [lastY(), view.originY, at(0.5), view.originY]) stroll(to)
        const swept = heights.changes
        verify(swept > 0, "the content height moved during the grouped scroll")
        compare(bands.changes, 0, "a grouped scroll whose content height moved " + swept + " times scheduled no look at the pool")
        bands.changes = 0
        view.flick(0, -view.maximumFlickVelocity)
        tryVerify(() => view.moving, 1000)
        tryVerify(() => !view.moving, 5000)
        compare(bands.changes, 2, "a flick schedules one look when it starts, which waits, and one when it stops")
        bands.changes = 0; heights.changes = 0
        for (let i = 0; i < 10; ++i) { tracks.append(entry(400 + i)); wait(50) }
        compare(bands.changes, 0, "a model growing by ten rows, one every 50 ms, scheduled no look at the pool")
        // One look within the budget lays nothing out and walks the rows the view holds.
        const runs = 500, start = Date.now()
        for (let i = 0; i < runs; ++i) looks.trim()
        const each = (Date.now() - start) / runs
        console.log("pool: grouped scroll " + swept + " content heights, 0 looks; flick 2 scheduled; growth "
            + heights.changes + " content heights, 0 looks; one look " + each.toFixed(3) + " ms over " + view.contentItem.children.length + " children")
    }

    // A body is built for the first time on the screen at once, in the margin asynchronously once its row is placed,
    // and never for the screen a jump lays out on its way. Every frame the window makes shows each row on the screen
    // with its body: the one made right after the call, before any turn of the event loop, and every one after it,
    // through a real flick too. Nothing is built for a row the list lets go. Each case logs what it built, when.
    function test_first_bodies_and_the_frames_that_show_them_data() {
        return [{ tag: "first fill" }, { tag: "taller list" }, { tag: "rich to compact" }, { tag: "scroll into a wider band" },
            { tag: "jumpTo" }, { tag: "jumpTo, grouped", grouped: true },
            { tag: "positionViewAtIndex" }, { tag: "positionViewAtIndex, grouped", grouped: true },
            { tag: "flick" }, { tag: "flick, grouped", grouped: true },
            { tag: "flick faster than a screen a frame", fast: true }, { tag: "flick faster than a screen a frame, grouped", grouped: true, fast: true }]
    }
    function test_first_bodies_and_the_frames_that_show_them(data) {
        if (data.grouped) { view.section.property = "group"; fill(400); view.positionViewAtBeginning(); view.forceLayout() }
        if (data.tag === "first fill") tracks.clear()
        if (data.tag === "rich to compact") { list.rich = true; view.forceLayout() }
        wait(0)
        // A cold pool: the list lets go of what it pooled.
        view.reuseItems = false; view.reuseItems = true
        wait(0)
        if (data.fast) view.maximumFlickVelocity = view.height * 100
        const had = new Set(slots().map(row => body(row)).filter(own => own !== null))
        const before = built(), top = view.contentY
        frames.made = 0; frames.emptiest = 0
        if (data.tag === "first fill") for (let i = 0; i < 400; ++i) tracks.append(entry(i))
        else if (data.tag === "taller list") list.height = 676
        else if (data.tag === "rich to compact") list.rich = false
        else if (data.tag === "scroll into a wider band") view.contentY = at(0.5)
        else if (data.tag.startsWith("jumpTo")) list.jumpTo(300)
        else if (data.tag.startsWith("flick")) view.flick(0, -view.maximumFlickVelocity)
        else list.positionAt(300, ListView.Beginning)
        const during = built() - before
        grabImage(test)
        const first = frames.emptiest
        if (data.tag.startsWith("flick")) tryVerify(() => !view.moving, 5000, "the flick ends")
        wait(300)
        if (data.tag.startsWith("flick")) verify(view.contentY > top + view.height * 2, "the flick went " + (view.contentY - top) + " px")
        const shown = slots().filter(row => row.visible && row.immediate)
        verify(shown.length > 0)
        for (const row of shown) checkRow(row)
        const all = built() - before
        const kept = slots().map(row => body(row)).filter(own => own !== null && !had.has(own)).length
        console.log(data.tag + ": " + during + " bodies built in the call, " + all + " in all, " + kept + " kept, " + (all - kept) + " thrown away; "
            + first + " rows on the screen without a body in the frame made right after the call, at most " + frames.emptiest + " in the " + frames.made + " frames made")
        compare(frames.emptiest, 0, "no frame showed a row on the screen without its body")
        compare(all - kept, 0, "no body was built for a row the list let go")
    }

    function test_keyboard_moves_show_rows_already_built() {
        warm()
        list.forceActiveFocus()
        list.viAction("first")
        for (const action of ["last", "first", "halfDown", "halfDown", "halfUp", "last", "halfUp", "first", "down", "up"]) {
            list.viAction(action)
            wait(0)
            const row = view.itemAtIndex(list.cursorRow)
            verify(row !== null && row.visible && row.immediate, action + " brings row " + list.cursorRow + " on the screen")
            for (const shown of slots().filter(slot => slot.visible && slot.immediate)) {
                verify(body(shown) !== null, action + ": row " + shown.index + " is drawn")
                checkRow(shown)
            }
            verify(slots().every(slot => slot.contentBuilds <= 1), action + ": no row built its body twice")
        }
    }

    function test_rows_that_moved_with_motion_scroll_into_place() {
        Motion.reduced = false
        warm()
        tracks.insert(4, Object.assign(entry(450), { number: 451 }))
        sweep(at(0.1)); sweep(view.originY)
        tracks.move(2, 9, 1)
        sweep(view.originY + view.height); sweep(view.originY)
        wait(Motion.panel * 3)
        const shown = slots().filter(row => row.visible && row.immediate).sort((a, b) => a.index - b.index)
        for (const row of shown) checkData(row)
        for (let i = 1; i < shown.length; ++i)
            compare(shown[i].y, shown[i - 1].y + shown[i - 1].height, "row " + shown[i].index + " sits right under row " + shown[i - 1].index)
        for (const row of shown.filter(row => !row.header))
            compare(surfaces().filter(surface => surface.visible && surface.y === row.y).length, 1, "row " + row.index + " has one surface at its place")
    }

    // A track that cannot play arrives by an insert, with motion on: it fades in to its dimmed opacity, and its row keeps
    // following whether its track can play, also as another such track in the same row, and when the motion stops
    // halfway through an arrival and settles it.
    function test_a_track_that_cannot_play_arrives_dimmed() {
        Motion.reduced = false
        warm()
        tracks.insert(2, entry(3))
        view.forceLayout()
        const row = view.itemAtIndex(2)
        verify(row !== null && !row.available)
        wait(Motion.panel * 3)
        compare(row.opacity, Metrics.disabledOpacity, "it arrived dimmed")
        tracks.set(2, entry(10))
        compare(view.itemAtIndex(2), row, "the same row object")
        compare(row.opacity, Metrics.disabledOpacity, "another track that cannot play in the same row stays dimmed")
        tracks.setProperty(2, "available", true)
        compare(row.opacity, 1, "its track can play")
        tracks.setProperty(2, "available", false)
        compare(row.opacity, Metrics.disabledOpacity, "and cannot again")
        tracks.remove(2)
        wait(Motion.panel * 3)
        tracks.insert(2, entry(3))
        view.forceLayout()
        wait(Motion.base / 3)
        const arriving = view.itemAtIndex(2)
        verify(arriving !== null && !arriving.available)
        Motion.reduced = true
        compare(arriving.opacity, Metrics.disabledOpacity, "the motion stopped, its arrival settles dimmed")
        tracks.setProperty(2, "available", true)
        compare(arriving.opacity, 1, "and its row still follows its track")
        tracks.setProperty(2, "available", false)
        compare(arriving.opacity, Metrics.disabledOpacity)
        tracks.remove(2)
    }

    function test_the_tracks_sheet_reuses_its_rows() {
        const page = createTemporaryObject(sheetPage, test)
        const sheetView = findChild(page.list, "trackListView"), cols = page.list.columns
        for (let i = 0; i < 300; ++i)
            page.model.append(Object.assign(entry(i), { bitDepth: i % 2 ? 24 : 16, sampleRate: 44100 + i * 100, format: "F" + i }))
        const rows = () => sheetView.contentItem.children.filter(item => item.contentBuilds !== undefined && item.rowKind !== undefined)
        const shownRows = () => rows().filter(row => row.visible && row.immediate)
        sheetView.forceLayout()
        for (const to of [sheetView.contentHeight / 4, 0]) {
            while (Math.abs(to - sheetView.contentY) > sheetView.height / 3) { sheetView.contentY += (to > sheetView.contentY ? 1 : -1) * sheetView.height / 3; sheetView.forceLayout(); wait(20) }
            sheetView.contentY = to; sheetView.forceLayout(); wait(20)
        }
        tryVerify(() => rows().every(row => findChild(row, "trackRowBody") !== null), 2000, "the sheet's rows have built their bodies")
        verify(page.contentWidth > page.width && !cols.bitsShown, "the technical columns start out of sight")
        const checkSheetRow = row => {
            const data = page.model.get(row.index)
            compare(row.rowKind, 0, "the sheet shows every row as a track")
            compare(findChild(row, "trackNumber").text, String(row.index + 1))
            compare(findChild(row, "creditedText").text, data.title)
            compare(row.sourceBadge.name, cols.sourceShown ? data.sourceName : "")
            compare(findChild(row, "trackBitDepth") !== null && findChild(row, "trackBitDepth").visible, cols.bitsShown, "row " + row.index + " shows its bit depth with its column")
            if (cols.bitsShown) compare(findChild(row, "trackBitDepth").text, data.bitDepth + "-bit")
            if (cols.rateShown) compare(findChild(row, "trackSampleRate").text, (data.sampleRate / 1000) + "kHz")
        }
        const builds = () => rows().reduce((sum, row) => sum + row.contentBuilds, 0)
        const warmBuilds = builds()
        for (const to of [sheetView.contentHeight - sheetView.height, 0, sheetView.contentHeight / 2]) {
            sheetView.contentY = to; sheetView.forceLayout()
            wait(0)
            for (const row of shownRows()) checkSheetRow(row)
        }
        compare(builds(), warmBuilds, "long scrolls on the sheet built no body")
        // Rich rows pool some; the technical columns come into sight: only the rows on the screen build them.
        page.list.rich = true
        sheetView.forceLayout(); wait(0)
        const pooled = rows().filter(row => !row.visible)
        verify(pooled.length > 0)
        page.contentX = page.contentWidth - page.width
        wait(0)
        verify(cols.bitsShown && cols.rateShown, "the technical columns are in sight")
        for (const row of pooled) compare(findChild(row, "trackBitDepth"), null, "pooled row " + row.index + " built no technical cells")
        for (const row of shownRows()) checkSheetRow(row)
        page.list.rich = false
        sheetView.forceLayout()
        wait(0)
        for (const row of shownRows()) checkSheetRow(row)
        // A still pointer over an artist; the sheet scrolls sideways under it, and the next frame hovers the album.
        page.contentX = 0
        sheetView.positionViewAtBeginning()
        waitForRendering(page)
        const row = shownRows().find(row => row.index === 3)
        const x = cols.artistX + Metrics.gap8, y = row.y - sheetView.contentY + row.height / 2
        mouseMove(sheetView, x, y)
        tryCompare(sheetView, "artistHovered", 3)
        page.contentX = cols.albumX - cols.artistX
        waitForRendering(page)
        compare(sheetView.artistHovered, -1, "the artist left the pointer")
        compare(sheetView.albumHovered, 3, "the album came under it")
    }
}
