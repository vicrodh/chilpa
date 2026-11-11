import QtQuick
import QtTest
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../../qml/Components"

// The column choosers name every column beside its switch, and a shelf's
// title keeps clear of its link and its chevrons, in each language, also a
// tenth wider than Inter sets them.
TestCase {
    id: test
    name: "ColumnsTextFit"
    when: windowShown && TextAudit.ready
    visible: true
    width: 960
    height: 700

    Component { id: gearFactory; AlbumColumnsGear { shown: ["year", "tracks", "duration", "rating", "source"] } }
    Component { id: headerFactory; SectionHeader { paged: true; canForward: true } }
    TechnicalColumns { id: technical }

    function cleanup() {
        I18n.choose("en")
        test.width = 960
    }

    // A switch draws no text, so the audit cannot see a label over one: each
    // label beside a switch must end before the switch starts.
    function besideSwitches(root) {
        const out = []
        TextAudit.walk(root, (toggle) => {
            if (!("knobInset" in toggle) || !TextAudit.shown(toggle)) return
            const left = toggle.mapToItem(root, 0, 0).x
            for (const sibling of toggle.parent.children) {
                if (!TextAudit.isText(sibling) || sibling.text === "" || !TextAudit.shown(sibling)) continue
                const box = TextAudit.painted(sibling)
                const at = sibling.mapToItem(root, box.x, 0).x
                if (at < left && at + box.width > left + TextAudit.slack)
                    out.push("\"" + sibling.text + "\" runs " + Math.ceil(at + box.width - left) + " px under its switch")
            }
        })
        return out
    }

    property var found: []
    function audit(root, data, where) {
        const more = TextAudit.check(test, root, data.scale).concat(besideSwitches(root))
        if (more.length > 0) found = found.concat([where + ":"]).concat(more)
    }

    function test_chooser_data() { return TextAudit.cases() }
    function test_chooser(data) {
        found = []
        I18n.choose(data.language)
        // The smallest window, and one narrower than any so a name must wrap.
        for (const room of [960, 320]) {
            test.width = room
            for (const options of [[], technical.choices]) {
                const gear = createTemporaryObject(gearFactory, test, { options: options })
                mouseClick(gear)
                const chooser = findChild(gear, "albumColumns")
                tryVerify(() => chooser.opened, 2000)
                audit(chooser.contentItem, data, (options.length ? "the technical" : "the list") + " chooser in a window " + room + " wide")
                chooser.close()
                tryVerify(() => !chooser.visible, 2000)
                gear.destroy()
                wait(0)
            }
        }
        compare(found.length, 0, found.join("\n"))
    }

    // The chooser is as wide as its longest name needs on one line, and no
    // narrower than the design's.
    function test_the_chooser_follows_its_names() {
        I18n.choose("fr")
        const gear = createTemporaryObject(gearFactory, test)
        mouseClick(gear)
        const chooser = findChild(gear, "albumColumns")
        tryVerify(() => chooser.opened, 2000)
        TextAudit.check(test, chooser.contentItem, TextAudit.wider)
        verify(chooser.width > Metrics.sleepTimer + Metrics.modalInsets, "Fréquence d’échantillonnage widens it: " + chooser.width)
        TextAudit.walk(chooser.contentItem, (item) => {
            if (TextAudit.isText(item)) compare(item.lineCount, 1, item.text + " stays on one line")
        })
        chooser.close()
        tryVerify(() => !chooser.visible, 2000)
        I18n.choose("en")
        tryCompare(chooser, "preferredWidth", Metrics.sleepTimer + Metrics.modalInsets)
    }

    function test_shelf_title_data() { return TextAudit.cases() }
    function test_shelf_title(data) {
        found = []
        I18n.choose(data.language)
        for (const wide of [692, 360]) {
            for (const title of [I18n.tr("Recently played playlists in the library", I18n.rev), I18n.tr("Most played tracks", I18n.rev)]) {
                const head = createTemporaryObject(headerFactory, test, { width: wide, title: title, link: I18n.tr("See all", I18n.rev) })
                audit(head, data, "a shelf's title " + wide + " wide")
                head.destroy()
            }
        }
        compare(found.length, 0, found.join("\n"))
    }
}
