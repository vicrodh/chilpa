import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import QtTest
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml" as Shell
import "qrc:/qt/qml/app/chilpa/sources/qml/Components" as Presentation

// The window's chrome says every word in each language without running into
// its neighbours, also a tenth wider than Inter sets it: the player bar and
// what it opens, the sidebar, the tabs and the context menus.
TestCase {
    id: test
    name: "ChromeTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1280
    height: 800

    readonly property string longTitle: "Concierto de Aranjuez: II. Adagio (Live at the Palau de la Música, 1984)"

    Component { id: wideBar; PlayerBar { width: 1100; y: 600 } }
    Component { id: narrowBar; PlayerBar { width: 780; y: 600 } }
    Component { id: tooltipFactory; QualityTooltip { x: 20; y: 20 } }
    Component { id: optionsFactory; PlaybackOptions { x: 20; y: 20 } }
    Component { id: ratingFactory; RatingButton { x: 40; y: 20; kind: "album"; target: 42 } }
    Component { id: toastFactory; Toast {} }
    Component { id: tooltipLayer; Tooltip {} }
    Component { id: sidebarFactory; Sidebar { width: Metrics.sidebarWidth; height: 780; collapsed: false } }
    Component { id: tabBarFactory; TabBar { width: 1100 } }
    Component { id: narrowTabBarFactory; TabBar { width: 940 } }
    Component { id: tabFactory; TabButton { x: 20; y: 20 } }
    Component { id: navMenuFactory; NavMenu { x: 20; y: 20 } }
    Component { id: trackListFactory; TrackListModel {} }
    Component { id: trackMenuFactory; TrackMenu {} }
    Component { id: albumMenuFactory; AlbumMenu {} }
    Component { id: queueMenuFactory; QueueMenu {} }
    Component { id: viewMenuFactory; Item { x: 900; y: 700; width: Metrics.button28; height: Metrics.button28; property alias menu: viewMenu; Presentation.PlayerViewMenu { id: viewMenu } } }
    Component { id: castPickerFactory; CastPicker { preview: true; x: 20; y: 20 } }
    // The top of the window as Main lays it out at the smallest window: the
    // tab bar between the margins and the sidebar frame over it.
    QtObject { id: router; property int generation: 0; property string phase: "content"; property string route: ""; signal preparingRoute(); function request(force) { generation++ } }
    Component {
        id: smallWindow
        Window {
            id: shell
            width: 960
            height: 600
            visible: true
            TabBar { x: Metrics.spaceWindow; width: shell.width - Metrics.spaceWindow * 2 }
            Shell.SidebarFrame { z: 2; anchors.fill: parent; window: shell; router: router }
            Tooltip {}
        }
    }

    function init() {
        Player.title = test.longTitle
        Player.artist = "Orquesta Sinfónica de la Radiotelevisión Española"
        Player.album = "Rodrigo: Concierto de Aranjuez · Fantasía para un gentilhombre"
        Player.aiGenerated = true
        Player.qualityTier = 3
        Player.playing = true
        Player.live = false
        Cast.active = false
        PlayerLayout.mode = "floating"
        SettingsModel.extraControls = true
        SettingsModel.sidebarMode = "expanded"
    }
    function cleanup() {
        closeMenus()
        I18n.choose("en")
        Player.title = "Fixture track"
        Player.artist = "Fixture artist"
        Player.album = "Fixture album"
        Player.aiGenerated = false
        Player.qualityTier = 2
        Player.bitPerfect = false
        Player.live = false
        Cast.active = false
        Cast.snapshot = "{}"
        Cast.destination = ""
        PlayerLayout.mode = "floating"
        SettingsModel.extraControls = false
        SettingsModel.sidebarMode = "expanded"
        Navigation.tabTitles = []
        Navigation.tabIcons = []
        Navigation.active = 0
        Sources.show({ panels: [{ id: "radio", name: "Radio", icon: "broadcast" }], download_plugins: [] })
        Ratings.albums = ({})
        TipState.hide(TipState.target)
    }

    function audit(root, data, options) {
        const found = TextAudit.check(test, root, data.scale, options)
        compare(found.length, 0, found.join("\n"))
    }
    // Every item under `root` that `match` takes.
    function every(root, match) {
        const out = []
        const stack = [root]
        while (stack.length > 0) {
            const item = stack.pop()
            if (!item) continue
            if (match(item)) out.push(item)
            for (const child of item.children || []) stack.push(child)
        }
        return out
    }
    // A popup open and laid out, audited where it is drawn.
    function auditPopup(popup, data) {
        tryCompare(popup, "opened", true)
        audit(popup.contentItem, data)
    }
    // A menu's entries say all their words: none is cut short where the
    // menu could have been as wide as it asked. Past the menu's widest, a
    // name is cut and its tooltip says it whole, by design.
    function auditMenu(menu, data) {
        test.menus = test.menus.concat([menu])
        tryCompare(menu, "opened", true)
        tryVerify(() => menu.entryItems.filter(item => item.entry !== undefined).length === menu.shown.length, 2000)
        audit(menu.contentItem, data)
        const room = menu.widthLimit - menu.leftPadding - menu.rightPadding
        const cut = menu.entryItems.filter(item => item.cut === true && item.implicitWidth <= room)
            .map(item => "\"" + item.entry.text + "\" is cut at " + Math.floor(item.width) + " px, needs " + Math.ceil(item.implicitWidth))
        compare(cut.length, 0, cut.join("\n"))
    }
    // The branch stays open: closing it closes the menu it hangs from.
    function openBranch(menu, index, data) {
        menu.showBranch(index, 0)
        const branch = findChild(menu, "menuBranch")
        tryVerify(() => branch.opened && branch.entries === menu.entries[index].children, 2000)
        auditMenu(branch, data)
    }
    // A menu left open when a check fails is closed before it goes.
    property var menus: []
    function closeMenus() {
        for (const menu of test.menus) if (menu) menu.close()
        test.menus = []
    }
    // A tab's buttons stay on the tab: one that spills sits on the next.
    function tabsHoldTheirButtons(root) {
        const out = []
        for (const tab of every(root, item => item.renamable !== undefined && item.sourceIcon !== undefined && item.visible))
            for (const button of every(tab, item => item !== tab && typeof item.tip === "string" && item.visible)) {
                const at = button.mapToItem(tab, 0, 0)
                if (at.x < -1 || at.x + button.width > tab.width + 1)
                    out.push("\"" + tab.title + "\": " + button.tip + " at " + Math.round(at.x) + ".." + Math.round(at.x + button.width) + " of " + Math.round(tab.width))
            }
        compare(out.length, 0, out.join("\n"))
    }

    // The player bar: transport, the song card with a long title, artist
    // and album, its badges and quality, and the actions.
    function test_player_bar_wide_data() { return TextAudit.cases() }
    function test_player_bar_wide(data) {
        I18n.choose(data.language)
        audit(createTemporaryObject(wideBar, test), data)
    }
    function test_player_bar_narrow_data() { return TextAudit.cases() }
    function test_player_bar_narrow(data) {
        I18n.choose(data.language)
        PlayerLayout.mode = "bottom"
        audit(createTemporaryObject(narrowBar, test), data)
    }
    // Casting a stream, at the window's top: no seek, no time left.
    function test_player_bar_casting_data() { return TextAudit.cases() }
    function test_player_bar_casting(data) {
        I18n.choose(data.language)
        PlayerLayout.mode = "top"
        Player.live = true
        Cast.snapshot = JSON.stringify({ icon: "dlna", protocol: "DLNA / UPnP", state: "Playing" })
        Cast.destination = "Living room receiver"
        Cast.active = true
        audit(createTemporaryObject(wideBar, test), data)
    }

    // What the quality glyph says when hovered, in each of its states.
    function test_quality_tooltip_data() { return TextAudit.cases() }
    function test_quality_tooltip(data) {
        I18n.choose(data.language)
        for (const state of [{ tier: 3, perfect: true, cast: false }, { tier: 0, perfect: false, cast: false }, { tier: 1, perfect: false, cast: false }, { tier: 2, perfect: false, cast: true }]) {
            Player.qualityTier = state.tier
            Player.bitPerfect = state.perfect
            Cast.snapshot = JSON.stringify({ icon: "cast", protocol: "Chromecast", state: "Playing" })
            Cast.destination = "Living room speaker"
            Cast.active = state.cast
            audit(createTemporaryObject(tooltipFactory, test), data)
        }
    }

    function test_playback_options_data() { return TextAudit.cases() }
    function test_playback_options(data) {
        I18n.choose(data.language)
        const options = createTemporaryObject(optionsFactory, test)
        options.open()
        auditPopup(options, data)
    }

    function test_rating_flyout_data() { return TextAudit.cases() }
    function test_rating_flyout(data) {
        I18n.choose(data.language)
        for (const stars of [0, 3]) {
            Ratings.albums = stars > 0 ? { 42: stars } : ({})
            Ratings.rev++
            const control = createTemporaryObject(ratingFactory, test)
            const flyout = findChild(control, "ratingFlyout")
            flyout.open()
            auditPopup(flyout, data)
            flyout.close()
        }
    }

    // Every tooltip the bar's buttons give, drawn where the window draws it.
    function test_player_tooltips_data() { return TextAudit.cases() }
    function test_player_tooltips(data) {
        I18n.choose(data.language)
        const bar = createTemporaryObject(wideBar, test)
        const tip = createTemporaryObject(tooltipLayer, test)
        const buttons = every(bar, item => typeof item.tip === "string" && item.tip !== "" && item.visible)
        verify(buttons.length >= 10, "the bar's buttons: " + buttons.length)
        for (const button of buttons) {
            TipState.show(button.tip, button)
            // Without the hover delay: what is measured is the tooltip.
            TipState.shown = true
            tryCompare(tip, "visible", true)
            audit(tip, data)
        }
    }

    // A line cut short says itself whole in the tooltip, kept inside the
    // smallest window however long it is.
    function test_long_tooltip_data() { return TextAudit.cases() }
    function test_long_tooltip(data) {
        I18n.choose(data.language)
        const shell = createTemporaryObject(smallWindow, test)
        tryVerify(() => shell.contentItem.width === 960, 2000)
        const anchor = every(shell.contentItem, item => item.tip === I18n.tr("Menu", I18n.rev))[0]
        // A cut menu line says its text and its detail: a movement and its performers.
        TipState.show("Symphony No. 9 in D minor, Op. 125 \"Choral\": IV. Presto – Allegro assai – Presto (\"O Freunde, nicht diese Töne!\") — Gwyneth Jones, Hanna Schwarz, René Kollo, Kurt Moll; Konzertvereinigung Wiener Staatsopernchor, Wiener Philharmoniker, Leonard Bernstein", anchor)
        TipState.shown = true
        const tip = every(shell.contentItem, item => item.watching !== undefined && item.target !== undefined)[0]
        tryCompare(tip, "visible", true)
        audit(shell.contentItem, data)
    }
    // The tab bar beside the sidebar in the smallest window, the active tab
    // last, as a library screen opens.
    function test_top_of_small_window_data() { return TextAudit.cases() }
    function test_top_of_small_window(data) {
        I18n.choose(data.language)
        Navigation.tabTitles = [I18n.tr("Tracks", I18n.rev), I18n.tr("Albums", I18n.rev)]
        Navigation.tabIcons = ["music", "disc"]
        for (const mode of ["expanded", "closed"]) for (const active of [0, 1]) {
            SettingsModel.sidebarMode = mode
            Navigation.active = active
            const shell = createTemporaryObject(smallWindow, test)
            tryVerify(() => shell.contentItem.width === 960, 2000)
            audit(shell.contentItem, data)
            tabsHoldTheirButtons(shell.contentItem)
        }
    }

    function test_toast_data() { return TextAudit.cases() }
    function test_toast(data) {
        I18n.choose(data.language)
        const toast = createTemporaryObject(toastFactory, test)
        toast.show(I18n.tr("Level loudness across tracks. This breaks strict bit perfect.", I18n.rev) + " " + test.longTitle, false)
        audit(toast, data)
    }

    // The sidebar at its own width: every destination, a source with its
    // children, the plugins' downloads and both section headings.
    function test_sidebar_data() { return TextAudit.cases() }
    function test_sidebar(data) {
        I18n.choose(data.language)
        Sources.show({ navigation_id: "collection", panels: [
            { id: "radio", name: "Internet radio stations", icon: "broadcast", children: [{ id: "collection", title: "Collection" }, { id: "favorites", title: "Favorite stations of the week" }] },
            { id: "podcasts", name: "Podcasts", icon: "microphone" }],
            download_plugins: [{ id: "purchases", name: "Purchased downloads", access: "sidebar" }] })
        const side = createTemporaryObject(sidebarFactory, test)
        side.expandedSources = { radio: true }
        tryVerify(() => every(side, item => item.objectName === "sourceChild" && item.visible).length === 2, 2000)
        audit(side, data)
    }

    // The tab bar with the sidebar open and closed (its menu, the playlists
    // button and the search join it), its tabs named as the pages are.
    function test_tab_bar_data() { return TextAudit.cases() }
    function test_tab_bar(data) {
        I18n.choose(data.language)
        Navigation.tabTitles = [I18n.tr("Library", I18n.rev), I18n.tr("Listening stats", I18n.rev), test.longTitle]
        Navigation.tabIcons = ["columns3", "chartBar", "disc"]
        Navigation.active = 1
        for (const mode of ["expanded", "closed"]) {
            SettingsModel.sidebarMode = mode
            const bar = createTemporaryObject(tabBarFactory, test)
            audit(bar, data)
            tabsHoldTheirButtons(bar)
        }
    }
    // More tabs than fit: the arrows come, the tabs scroll.
    function test_tab_bar_crowded_data() { return TextAudit.cases() }
    function test_tab_bar_crowded(data) {
        I18n.choose(data.language)
        const names = ["Home", "Library", "Artists", "Albums", "Tracks", "Folders", "Open media", "Devices", "Settings", "Keyboard shortcuts"]
        Navigation.tabTitles = names.map(name => I18n.tr(name, I18n.rev))
        Navigation.tabIcons = names.map(() => "")
        Navigation.active = 9
        SettingsModel.sidebarMode = "closed"
        const bar = createTemporaryObject(narrowTabBarFactory, test)
        audit(bar, data)
        tabsHoldTheirButtons(bar)
    }
    // A tab that can be renamed, active: its pencil and its close button.
    function test_renamable_tab_data() { return TextAudit.cases() }
    function test_renamable_tab(data) {
        I18n.choose(data.language)
        for (const title of [I18n.tr("Keyboard shortcuts", I18n.rev), test.longTitle]) {
            const holder = createTemporaryObject(tabRow, test, { title: title })
            audit(holder, data)
            tabsHoldTheirButtons(holder)
        }
    }
    // The tab beside the one being checked: a button that spills runs into it.
    Component {
        id: tabRow
        Row {
            property string title
            x: 20; y: 20
            spacing: Metrics.gap4
            TabButton { title: parent.title; glyph: Icons.list; renamable: true; active: true }
            TabButton { title: I18n.tr("Library", I18n.rev); glyph: Icons.columns3 }
        }
    }

    // The app's menu from the tab bar and its Updates branch.
    function test_main_menu_data() { return TextAudit.cases() }
    function test_main_menu(data) {
        I18n.choose(data.language)
        const bar = createTemporaryObject(tabBarFactory, test)
        const more = every(bar, item => item.tip === I18n.tr("Menu", I18n.rev))[0]
        verify(more !== undefined)
        const menu = more.data.find(child => child.actionChosen !== undefined && child.entries !== undefined)
        verify(menu !== undefined)
        menu.open()
        auditMenu(menu, data)
        openBranch(menu, menu.entries.findIndex(entry => entry.submenu === true), data)
        menu.close()
    }

    // The destinations' menu with the sidebar closed.
    function test_navigation_menu_data() { return TextAudit.cases() }
    function test_navigation_menu(data) {
        I18n.choose(data.language)
        const button = createTemporaryObject(navMenuFactory, test)
        const menu = findChild(button, "navigationMenu")
        menu.open()
        auditMenu(menu, data)
        menu.close()
    }

    // A track's menu as the facade builds it for one track in an editable
    // playlist and for several, with the extra queues' branch.
    function test_track_menu_data() { return TextAudit.cases() }
    function test_track_menu(data) {
        I18n.choose(data.language)
        const t = text => I18n.tr(text, I18n.rev)
        const queues = { text: t("Extra queues"), submenu: true, children: [
            { act: "buffer:new", text: t("Open in new queue") },
            { act: "buffer:add:2", text: I18n.trArgs("Add to queue {}", ["Late night listening"], I18n.rev) },
            { act: "buffer:replace:2", text: I18n.trArgs("Replace queue {}", ["Late night listening"], I18n.rev) }] }
        const one = [
            { act: "play", text: t("Play now"), glyph: Icons.playerPlayFilled, filled: true },
            { act: "queue", text: t("Add to queue"), glyph: Lucide.listEnd, family: Lucide.font },
            { act: "next", text: t("Play next"), glyph: Lucide.listStart, family: Lucide.font },
            { act: "later", text: t("Play later"), glyph: Lucide.listPlus, family: Lucide.font },
            { separator: true },
            { act: "pick", text: t("Add to playlist…"), glyph: Icons.playlistAdd },
            { act: "sync", text: t("Sync to device…"), glyph: Icons.deviceMobile },
            { separator: true },
            { act: "album", text: t("Go to album"), glyph: Icons.disc },
            { act: "artist:0", text: I18n.trArgs("Go to {}", ["Orquesta Sinfónica de la Radiotelevisión Española"], I18n.rev), glyph: SourceGlyphs.musicArtist, family: SourceGlyphs.font },
            { act: "folder", text: t("Show in folder"), glyph: Icons.folder },
            { act: "metadata", text: t("Edit metadata…") },
            { separator: true },
            { act: "up", text: t("Move up"), hint: "Alt ↑", glyph: Icons.chevronUp },
            { act: "down", text: t("Move down"), hint: "Alt ↓", glyph: Icons.chevronDown },
            { act: "remove", text: t("Remove from playlist"), glyph: Icons.trash },
            { separator: true },
            { act: "server_delete", text: t("Delete from server…"), glyph: Icons.trash },
            { separator: true },
            { act: "offline", text: t("Make available offline…") },
            { separator: true }, queues]
        const several = [{ text: I18n.trCount("{} track", "{} tracks", 12, I18n.rev), note: true }].concat(one.slice(0, 7)).concat([{ separator: true }, queues])
        const tracks = createTemporaryObject(trackListFactory, test)
        for (const entries of [one, several]) {
            const menu = createTemporaryObject(trackMenuFactory, test, { model: tracks })
            menu.entries = entries
            menu.x = 40
            menu.y = 20
            menu.open()
            auditMenu(menu, data)
            openBranch(menu, entries.indexOf(queues), data)
            menu.close()
        }
    }

    // A record's menu, alone and as part of a selection.
    function test_album_menu_data() { return TextAudit.cases() }
    function test_album_menu(data) {
        I18n.choose(data.language)
        for (const selection of [[], [7, 9, 11]]) {
            AlbumActions.selectedIds = selection
            AlbumActions.selectedCount = selection.length
            const menu = createTemporaryObject(albumMenuFactory, test, { canExpand: true })
            menu.show(7, test.longTitle, "Orquesta Sinfónica de la Radiotelevisión Española", 0, 40, 20)
            auditMenu(menu, data)
            menu.close()
        }
        AlbumActions.selectedIds = []
        AlbumActions.selectedCount = 0
    }

    // A collection's queue menu, with the playlist's own entries.
    function test_queue_menu_data() { return TextAudit.cases() }
    function test_queue_menu(data) {
        I18n.choose(data.language)
        const tracks = createTemporaryObject(trackListFactory, test)
        const menu = createTemporaryObject(queueMenuFactory, test, { model: tracks, canRemove: true })
        menu.show(40, 20)
        // The facade's words, in the language chosen.
        menu.entries = menu.entries.map(entry => Object.assign({}, entry, entry.text ? { text: I18n.tr(entry.text, I18n.rev) } : {},
            entry.children ? { children: entry.children.map(child => Object.assign({}, child, { text: I18n.tr(child.text, I18n.rev) })) } : {}))
        auditMenu(menu, data)
        openBranch(menu, menu.entries.findIndex(entry => entry.submenu === true), data)
        menu.close()
    }

    // The player's view menu and its two branches.
    function test_view_menu_data() { return TextAudit.cases() }
    function test_view_menu(data) {
        I18n.choose(data.language)
        const holder = createTemporaryObject(viewMenuFactory, test)
        const menu = holder.menu
        menu.show()
        auditMenu(menu, data)
        for (let index = 0; index < menu.entries.length; ++index)
            if (menu.entries[index].submenu === true) openBranch(menu, index, data)
        menu.close()
    }

    // Where the music plays: a title, notes, details and a footer.
    function test_cast_picker_data() { return TextAudit.cases() }
    function test_cast_picker(data) {
        I18n.choose(data.language)
        const picker = createTemporaryObject(castPickerFactory, test)
        picker.open()
        auditMenu(picker, data)
        picker.close()
    }
}
