import QtQuick
import QtTest
import app.chilpa.kit
import app.chilpa.controls
import "qrc:/qt/qml/app/chilpa/qml/Components"
import "qrc:/qt/qml/app/chilpa/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/controls/qml/Patterns"
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"
// Qt 6.10 brings a SearchField of its own with QtQuick.Controls; this is the kit's.
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives" as Kit

// Every row Settings draws says its title, what it does, its choices and its
// button in each language without running into its control or its
// neighbours, at the page's width in a usual window and in the smallest one,
// also a tenth wider than Inter sets it. The rows are read from the facade's
// schema, so a setting added there is audited here too.
TestCase {
    id: test
    name: "SettingsTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 1100
    height: 900

    // A row's width in the settings list (SettingsScreen, Main): the window
    // less the sidebar and its margins, the page's margins, the section list
    // and the gap beside it, and the row's own inset.
    function rowWidth(window) {
        return window - Metrics.spaceWindow * 3 - Metrics.sidebarWidth - Metrics.contentX * 2 - Metrics.settingsNav - Metrics.heroGap - Metrics.gap12 // derived: SettingsScreen's list in Main's screen area
    }
    readonly property real usual: rowWidth(1280)
    readonly property real smallest: rowWidth(Metrics.windowMinWidth)

    QtObject {
        id: actions
        property string editingServerKey: ""
        function set(key, value) {}
        function press(key) {}
    }
    Component { id: listFactory; Column {} }
    Component { id: rowFactory; SettingsRow {} }
    Component {
        id: navFactory
        Column {
            width: Metrics.settingsNav
            spacing: Metrics.gap12
            Kit.SearchField { width: parent.width; placeholder: I18n.tr("Search settings", I18n.rev) }
            Repeater {
                model: test.sections()
                NavItem { required property string modelData; objectName: "section"; width: Metrics.settingsNav; text: I18n.tr(modelData, I18n.rev); glyph: Icons.list }
            }
        }
    }
    // The player's two option popovers: where OptionRow and ChoiceRow live.
    Component { id: lyricsFactory; LyricsOptions { store: QtObject { function kept(key) { return "" } function keep(key, value) {} } } }
    Component { id: playbackFactory; PlaybackOptions {} }

    function read(path) {
        const request = new XMLHttpRequest()
        request.open("GET", Qt.resolvedUrl(path), false)
        request.send()
        verify(request.responseText !== "", "read " + path)
        return request.responseText
    }
    // Every match of a global expression; the window's JavaScript has no matchAll.
    function each(text, expression) {
        const out = []
        for (let m = expression.exec(text); m; m = expression.exec(text)) out.push(m)
        return out
    }
    function unquote(text) { return text === undefined ? "" : JSON.parse("\"" + text + "\"") }
    readonly property string api: "../../../chilpa-api/src/"

    function sections() {
        const text = read(api + "settings_schema.rs")
        return each(text, /Section\s*\{\s*id:\s*"\w+",\s*label:\s*Chilpa::mark\("((?:[^"\\]|\\.)*)"\)/g).map(m => unquote(m[1]))
    }

    // The schema in the order Settings shows it: each group's title, then
    // its settings with their control, title, description and button. The
    // sections other screens draw (servers, plugins, developer tools) are
    // left out.
    function schema() {
        const text = read(api + "settings_schema.rs")
        const table = text.slice(text.indexOf("pub const SECTIONS"), text.indexOf("pub fn section("))
        const entry = /Section\s*\{\s*id:\s*"(\w+)"|Group\s*\{\s*title:\s*(?:Chilpa::mark\()?"((?:[^"\\]|\\.)*)"|\b(?:setting|action)\(\s*([\w:."]+)\s*,\s*(?:Chilpa::mark\()?"((?:[^"\\]|\\.)*)"\)?\s*,\s*(?:Chilpa::mark\()?"((?:[^"\\]|\\.)*)"\)?\s*,\s*Control::(\w+)(?:\s*,\s*Chilpa::mark\("((?:[^"\\]|\\.)*)"\))?/g
        const out = []
        let section = "", settings = 0
        for (const m of each(table, entry)) {
            if (m[1]) { section = m[1]; continue }
            const drawn = !["sources", "plugins", "developers"].includes(section)
            if (m[2] !== undefined) {
                if (drawn) out.push({ rowKind: 0, key: "", title: unquote(m[2]), description: "", control: "", action: "" })
                continue
            }
            settings++
            if (drawn) out.push({ rowKind: 1, key: m[3].replace(/"/g, ""), source: m[3], title: unquote(m[4]), description: unquote(m[5]), control: m[6].toLowerCase(), action: unquote(m[7]) })
        }
        compare(settings, (table.match(/Control::\w+/g) || []).length, "every setting of the schema was read")
        return out
    }

    // The labels of a select or segmented setting, as settings_rows.rs
    // `fixed` and the features' choice tables give them, or, for what the
    // machine decides (stacks, outputs, mixer controls, GPUs), a sample.
    function choices() {
        const rows = read(api + "settings_rows.rs")
        const fixed = rows.slice(rows.indexOf("pub(crate) fn fixed"), rows.indexOf("const DEFAULTS"))
        const label = /chilpa_i18n::(?:t|mark)\("((?:[^"\\]|\\.)*)"\)|"((?:[^"\\]|\\.)*)"\.(?:into|to_string)\(\)/g
        const out = {}
        for (const arm of each(fixed, /("[\w.]+"|crate::[\w:]+)\s*=>\s*\{?\s*vec!\[([^\]]*)\]/g))
            out[arm[1].replace(/"/g, "")] = each(arm[2], label).map(m => m[1] !== undefined ? I18n.tr(unquote(m[1])) : unquote(m[2]))
        const tables = { "crate::graphics_boot::RENDERER": ["graphics_boot.rs", "RENDERER_CHOICES"], "crate::graphics_boot::INTERFACE_SIZE": ["graphics_boot.rs", "SIZE_CHOICES"],
            "crate::queue::normalization::TARGET": ["queue/normalization.rs", "TARGET_CHOICES"], "crate::queue::back_button::BACK": ["queue/back_button.rs", "CHOICES"], "crate::outputs::channel_mode::CHANNEL_MODE": ["outputs/channel_mode.rs", "CHOICES"],
            "integrations.discord.artworkSource": ["discord/mod.rs", "SOURCES"], "integrations.discord.artworkRetention": ["discord/mod.rs", "RETENTIONS"],
            "integrations.discord.memberList": ["discord/mod.rs", "MEMBER_LISTS"] }
        for (const key in tables) {
            const source = read(api + tables[key][0])
            const body = source.match(new RegExp("const " + tables[key][1] + ":[^=]*=\\s*\\[([^;]*)\\];"))
            verify(body, key + " has its choices")
            out[key] = each(body[1], label).map(m => I18n.tr(unquote(m[1])))
        }
        const through = "(" + I18n.trArgs("through {}", ["PipeWire"]) + ")"
        out["audio.backend"] = [I18n.tr("System"), "ALSA", "PipeWire", "PulseAudio " + through, "JACK " + through]
        out["audio.device"] = ["USB DAC, USB Audio", "HDA Intel PCH, ALC1220 Analog", "HDMI 0 (External monitor), HDMI Audio"]
        out["audio.alsaPlugin"] = ["hw: the card alone", "plughw: converts what it refuses", "pcm: shared, as ALSA sets it up"].map(t => I18n.tr(t))
        out["audio.hwVolumeControl"] = [I18n.tr("Choose a playback control"), "PCM", "Digital Playback Volume"]
        out["crate::beets_runtime::ENGINE"] = ["Chilpa", "beets"]
        out["crate::graphics_boot::gpu::GPU"] = [I18n.tr("Auto (recommended)"), "AMD Radeon RX 7900 XTX (RADV NAVI31)", "Intel Arc A770 Graphics (DG2)"]
        return out
    }

    // States the facade puts a row in while something happens: a long
    // button, a pill with its menu, a field with its button, a job under
    // way, a report under a setting, a choice that cannot be made here.
    function states() {
        const tr = (text) => I18n.tr(text)
        return [
            { rowKind: 0, key: "", title: tr("Application"), description: "", control: "", action: "" },
            { rowKind: 1, key: "updates.check", title: tr("Check for updates now"), description: I18n.trArgs("Version {} is available.", ["0.1.3"]), control: "buttonBusy", action: tr("Preparing the updated AppImage…") },
            { rowKind: 2, key: "updates.installation", title: tr("Installation"), description: "", control: "none", action: "", value: tr("System / manual") },
            { rowKind: 1, key: "integrations.lastfm.account", title: I18n.trArgs("Connected as {}", ["listener"]), description: I18n.trCount("{} listen waiting to be sent", "{} listens waiting to be sent", 12), control: "button", action: tr("Disconnect") },
            { rowKind: 1, key: "integrations.discogs.account", title: tr("Discogs account"), description: tr("Allow Chilpa on Discogs, then paste the code below."), control: "external", action: tr("Open Discogs") },
            { rowKind: 1, key: "sources.plugin/example", title: "Example source", description: I18n.trArgs("Requests: {}", ["example.org"]) + " " + tr("This update asks for additional access."), control: "status", action: tr("Awaiting approval"), marks: ["warning"],
              options: [tr("Approve new access"), tr("Withdraw approval")] },
            { rowKind: 1, key: "sources.token/example", title: I18n.trArgs("Connect {}", ["Example source"]), description: tr("Paste the token supplied by the source. Chilpa keeps it encrypted and the plugin never sees it."), control: "secret", action: tr("Connect") },
            { rowKind: 1, key: "library.analysis", title: tr("Analyze the library"), description: I18n.trArgs("Analyzing {} of {} tracks", ["12345", "67890"]), control: "progress", action: tr("Cancel"), value: "0.4" },
            { rowKind: 1, key: "audio.wizard", title: tr("Diagnose the output"), description: tr("Queries supported formats and predicts delivery. This does not test audible output."), control: "button", action: tr("Diagnose") },
            { rowKind: 2, key: "", title: "PCM 192 kHz · 24 bit", description: "S32_LE", control: "", action: "", value: "bit-perfect" },
            { rowKind: 1, key: "audio.dsdMode", title: tr("DSD playback"), description: tr("This output takes DSD converted to PCM. Only a card on hw takes it untouched; what is chosen for the card is kept."), control: "fixed", action: "",
              options: [tr("Native DSD"), tr("DSD over PCM (DoP)"), tr("Convert to PCM")] },
            { rowKind: 1, key: "storage.remote", title: tr("Remote artwork"), description: tr("Images from sources and plugins. Covers saved in your library stay protected.") + " · " + I18n.trArgs("{} files", ["12345"]) + " · " + I18n.trArgs("{} protected by the library", ["123.4 MiB"]), control: "none", action: "", value: tr("Calculating…") }
        ]
    }

    // Every row as the page model gives it, translated, each choice showing
    // its longest label.
    function rows() {
        const labels = choices()
        const out = schema().map(row => {
            const filled = Object.assign({}, row, { title: I18n.tr(row.title), description: I18n.tr(row.description), action: I18n.tr(row.action), value: "", options: [], optionValues: [], marks: [] })
            if (["select", "device", "segmented"].includes(row.control)) {
                filled.options = labels[row.source.replace(/"/g, "")] || []
                verify(filled.options.length > 0, row.key + " has choices to show")
            } else if (row.control === "swatches") filled.options = ["", "#A3161F", "#D8323A", "#3DAEE9", "#5C6166"]
            else if (row.control === "status") {
                // Usage and versions are read out, not chosen.
                filled.control = "none"
                filled.value = I18n.tr("Calculating…")
            } else if (row.control === "slider") filled.value = "100"
            else if (row.control === "theme") filled.value = "system"
            delete filled.source
            return filled
        }).concat(states().map(row => Object.assign({ value: "", options: [], marks: [] }, row)))
        for (const row of out) {
            row.optionValues = row.options.map((text, i) => String(i))
            if (row.marks.length === 0 && row.control === "device") row.marks = row.options.map((text, i) => i === 0 ? "BP" : "")
            if (row.options.length > 0 && row.control !== "swatches" && row.control !== "status")
                row.value = String(row.options.reduce((longest, text, i) => text.length > row.options[longest].length ? i : longest, 0))
        }
        return out
    }

    function build(width) {
        const list = createTemporaryObject(listFactory, test, { width: width })
        for (const row of rows()) {
            if (row.key === "navigation.playerLayout") PlayerLayout.mode = row.value
            const made = rowFactory.createObject(list, Object.assign({ width: width, page: actions }, row))
            verify(made !== null, "built " + row.key)
        }
        return list
    }
    function all(root, prefix) {
        const out = []
        const walk = (item) => { for (const child of item.children) { if (child.objectName.startsWith(prefix)) out.push(child); walk(child) } }
        walk(root)
        return out
    }

    function cleanup() {
        I18n.choose("en")
        PlayerLayout.mode = "floating"
    }

    // A text laid out again by words alone, to tell a word broken in its
    // middle from a line that wrapped between words.
    Text { id: byWords; visible: false; wrapMode: Text.WordWrap }
    // Wrapped texts too narrow for one of their words: past the audit, which
    // takes a word broken in two as fitting.
    function brokenWords(root) {
        const out = []
        TextAudit.walk(root, (item) => {
            if (!TextAudit.isText(item) || item.text === "" || item.wrapMode === Text.NoWrap || item.width <= 0 || !TextAudit.shown(item)) return
            byWords.font = item.font
            byWords.textFormat = item.textFormat
            byWords.text = item.text
            byWords.width = item.width - (item.leftPadding || 0) - (item.rightPadding || 0)
            if (byWords.contentWidth > Math.max(byWords.width, item.contentWidth) + TextAudit.slack)
                out.push(TextAudit.name(item, root) + " breaks a word: needs " + Math.ceil(byWords.contentWidth) + " px and has " + Math.floor(byWords.width))
        })
        return out
    }
    // The selects whose choices are product copy; outputs and GPUs are named
    // by the machine, and a long one may be cut.
    function worded(select) { return !/(audio\.device|::GPU)$/.test(select.objectName) }
    // These Japanese descriptions, wrapped at the widths they get here, report
    // a contentWidth 2.00 px past their box, yet each of their lines laid out
    // alone fits it (391.67 and 391.44 in 392, 347.70 in 348, 711.39 in 712): Qt adds
    // the 2 px to the whole text's width, not to any line's glyphs, and
    // nothing is drawn past the box.
    readonly property var wrappedWider: ["Look up lyrics through Chilpa's lyrics service when the file and its source have none.",
        "Remove unreferenced images from sources and plugins. Needed images download again.",
        "Chilpa sends no telemetry. When enabled, artist and release searches make one-way requests to MusicBrainz for results. Turning it off disables MusicBrainz features in Artist Network.",
        "Chilpa never changes your library on its own: every change goes through you. To rule out edits and anything that can't be undone, turn this on. It blocks tag and cover edits, deletions, changes to server playlists and syncing to a device whose folder is in your library. Favorites, ratings and adding sources keep working."]

    function fits(width, data) {
        I18n.choose(data.language)
        const list = build(width)
        const ignore = (item) => data.language === "ja" && test.wrappedWider.some(text => item.text === I18n.tr(text))
        const found = TextAudit.check(test, list, data.scale, { ignore: ignore }).concat(brokenWords(list))
        for (const select of all(list, "settingSelect.").filter(worded)) {
            const said = findChild(select, "advisoryText")
            if (said.truncated) found.push(select.objectName + ": \"" + said.text + "\" is cut in its field")
        }
        compare(found.length, 0, found.join("\n"))
    }

    function test_usual_window_data() { return TextAudit.cases() }
    function test_usual_window(data) { fits(test.usual, data) }
    function test_smallest_window_data() { return TextAudit.cases() }
    function test_smallest_window(data) { fits(test.smallest, data) }

    // The ratings switch (Library > Ratings), read from the schema like every
    // row above: whole in each language at both widths, and said in the
    // language's own words.
    function test_ratings_switch_data() { return TextAudit.cases() }
    function test_ratings_switch(data) {
        I18n.choose(data.language)
        const read = schema().find(row => row.key === "crate::rating_switch::ENABLED")
        verify(read !== undefined, "the switch is read from the schema")
        compare(read.control, "toggle")
        if (data.language !== "en")
            verify(I18n.tr(read.title) !== read.title && I18n.tr(read.description) !== read.description, "the switch is translated")
        const row = Object.assign({}, read, { title: I18n.tr(read.title), description: I18n.tr(read.description), value: "true", options: [], optionValues: [], marks: [] })
        delete row.source
        for (const width of [test.usual, test.smallest]) {
            const list = createTemporaryObject(listFactory, test, { width: width })
            verify(rowFactory.createObject(list, Object.assign({ width: width, page: actions }, row)) !== null)
            const found = TextAudit.check(test, list, data.scale).concat(brokenWords(list))
            compare(found.length, 0, found.join("\n"))
        }
    }

    // A row whose control would leave its texts less room than it takes goes
    // under them in the smallest window, and stays beside them in a usual one;
    // a switch never needs to.
    function test_a_narrow_row_takes_its_control_under_its_texts() {
        I18n.choose("de")
        const narrow = build(test.smallest), usual = build(test.usual)
        const row = (list, key) => Array.from(list.children).find(made => made.key === key)
        for (const [key, control] of [["integrations.listenbrainz.account", null], ["audio.alsaPlugin", "settingSelect.audio.alsaPlugin"]]) {
            verify(row(narrow, key).stacked, key + " goes under its texts")
            verify(!row(usual, key).stacked, key + " stays beside its texts")
            if (!control) continue
            const texts = Array.from(row(narrow, key).children).find(child => "least" in child)
            const field = findChild(row(narrow, key), control)
            verify(field.mapToItem(narrow, 0, 0).y >= texts.mapToItem(narrow, 0, texts.height).y, key + " sits under its texts")
            compare(field.width, field.wholeWidth, key + " shows its longest choice whole")
        }
        verify(!row(narrow, "audio.reserve").stacked, "a switch stays beside its texts")
    }

    // Each select's list of choices, opened, in both windows: how wide the
    // list may grow follows the row.
    function test_choices_data() { return TextAudit.cases() }
    function test_choices(data) {
        I18n.choose(data.language)
        const selects = all(build(test.usual), "settingSelect.").concat(all(build(test.smallest), "settingSelect.")).filter(select => select.enabled)
        verify(selects.length > 20, "the selects were built")
        const found = []
        for (const select of selects) {
            const menu = findChild(select, "selectOptions")
            select.show()
            tryCompare(menu, "opened", true)
            found.push(...TextAudit.check(test, menu.contentItem, data.scale).map(line => select.objectName + ": " + line))
            if (worded(select))
                found.push(...menu.entryItems.filter(entry => entry.cut).map(entry => select.objectName + ": \"" + entry.entry.text + "\" is cut in the list"))
            menu.close()
            tryCompare(menu, "visible", false)
        }
        compare(found.length, 0, found.join("\n"))
    }

    // OptionRow and ChoiceRow where the player's options show them: each
    // name whole beside its switch or its choice. What the popovers set
    // around them (the translation select's width, the footer) is theirs.
    function test_player_options_data() { return TextAudit.cases() }
    function test_player_options(data) {
        I18n.choose(data.language)
        const found = []
        for (const factory of [lyricsFactory, playbackFactory]) {
            const flyout = createTemporaryObject(factory, test)
            flyout.open()
            tryCompare(flyout, "opened", true)
            const rows = Array.from(flyout.contentItem.children).filter(row => "title" in row && ("available" in row || "control" in row))
            verify(rows.length >= 2, "the option rows were built")
            for (const row of rows) {
                found.push(...TextAudit.check(test, row, data.scale), ...brokenWords(row))
                found.push(...all(row, "").filter(text => TextAudit.isText(text) && text.truncated && text.objectName !== "advisoryText").map(text => "\"" + text.text + "\" is cut"))
            }
            flyout.close()
            tryCompare(flyout, "visible", false)
        }
        compare(found.length, 0, found.join("\n"))
    }

    // The lyrics options whole: the translation select says its choice
    // uncut, and the footer's buttons stay inside the column.
    function test_lyrics_options_data() { return TextAudit.cases() }
    function test_lyrics_options(data) {
        I18n.choose(data.language)
        const flyout = createTemporaryObject(lyricsFactory, test)
        flyout.open()
        tryCompare(flyout, "opened", true)
        const found = TextAudit.check(test, flyout.contentItem, data.scale)
        found.push(...all(flyout.contentItem, "").filter(text => TextAudit.isText(text) && text.truncated).map(text => "\"" + text.text + "\" is cut"))
        compare(found.length, 0, found.join("\n"))
        flyout.close()
        tryCompare(flyout, "visible", false)
    }

    // The section list and the search over it, in the column they get.
    function test_sections_data() { return TextAudit.cases() }
    function test_sections(data) {
        I18n.choose(data.language)
        const nav = createTemporaryObject(navFactory, test)
        compare(all(nav, "section").length, test.sections().length, "every section is listed")
        const found = TextAudit.check(test, nav, data.scale)
        compare(found.length, 0, found.join("\n"))
    }
}
