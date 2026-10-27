import QtQuick
import QtTest
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"
import "qrc:/qt/qml/app/chilpa/kit/qml/Components"

TestCase {
    id: test
    name: "ExplicitTitles"
    when: windowShown
    visible: true
    width: 700; height: 500
    AdvisoryTitle { id: title; width: 260; text: "A & <b>B</b> > C" }
    CreditedTitle { id: credit; y: 50; width: 260; title: title.text; credit: "A & <B>" }
    SelectField { id: ordinary; y: 100; options: ["First", "Second"]; values: ["a", "b"]; marks: ["BP", "E"]; value: "a"; onPicked: value => ordinary.value = value }
    QtObject {
        id: detail
        property int selectedId: 1
        property string versions: JSON.stringify([{id:1,label:"Same title",brief:"Same",explicit:true},{id:2,label:"Same title",brief:"Same",explicit:false},{id:3,label:"Same title",brief:"Same"}])
        function selectVersion(value) { selectedId = Number(value) }
    }
    TechnicalCells { id: technical; y: 240; width: 2000; height: 40; title: "A & <B>"; explicitContent: true }
    AlbumVersionSelect { id: versions; y: 160; detail: detail }
    function cleanup() {
        title.explicitContent = false; title.width = 260; title.horizontalAlignment = Text.AlignLeft
        credit.explicitContent = false; detail.selectedId = 1
        findChild(ordinary, "selectOptions").close()
        findChild(versions.item, "selectOptions").close()
        wait(0)
    }
    function test_plain_title_badge_lifecycle_and_elision() {
        const text = findChild(title, "advisoryText")
        compare(text.textFormat, Text.PlainText)
        compare(text.text, "A & <b>B</b> > C")
        tryVerify(() => findChild(title, "explicitBadge") === null)
        title.explicitContent = true
        const badge = findChild(title, "explicitBadge")
        verify(badge)
        compare(badge.Accessible.name, "Explicit")
        verify(badge.parent.x >= text.x + text.paintedWidth)
        title.width = 60
        verify(title.truncated)
        verify(badge.parent.x + badge.width <= title.width)
        compare(title.text, "A & <b>B</b> > C")
        title.explicitContent = false
        tryVerify(() => findChild(title, "explicitBadge") === null)
    }
    function test_centered_badge_follows_the_title_and_credit_keeps_its_baseline() {
        title.explicitContent = true; title.horizontalAlignment = Text.AlignHCenter
        const text = findChild(title, "advisoryText"), badge = findChild(title, "explicitBadge")
        verify(text.x > 0)
        verify(badge.parent.x >= text.x + text.paintedWidth)
        credit.explicitContent = true
        const name = findChild(credit, "creditedText"), composer = findChild(credit, "titleCredit")
        compare(composer.textFormat, Text.PlainText)
        compare(composer.text, "A & <B>")
        compare(name.y + name.baselineOffset, composer.y + composer.baselineOffset)
        verify(composer.x >= name.width + findChild(credit, "explicitBadge").width)
    }
    function test_versions_keep_equal_titles_distinct_without_mutating_labels() {
        const field = versions.item, menu = findChild(field, "selectOptions")
        compare(field.options, ["Same title", "Same title", "Same title"])
        verify(findChild(field, "explicitBadge") !== null)
        menu.open(); tryCompare(menu, "opened", true)
        const options = findChild(menu, "menuOptions")
        tryVerify(() => options.itemAtIndex(2) !== null)
        verify(findChild(options.itemAtIndex(0), "explicitBadge") !== null)
        compare(findChild(options.itemAtIndex(1), "explicitBadge"), null)
        compare(findChild(options.itemAtIndex(2), "explicitBadge"), null)
        mouseClick(options.itemAtIndex(1), 60, 10)
        compare(detail.selectedId, 2)
        tryVerify(() => findChild(field, "explicitBadge") === null)
        compare(field.options[1], "Same title")
    }
    function test_technical_album_title_uses_the_same_advisory() {
        const title = findChild(technical, "technicalCell_album")
        verify(title)
        verify(findChild(title, "explicitBadge") !== null)
        compare(findChild(title, "advisoryText").textFormat, Text.PlainText)
        technical.explicitContent = false
        tryVerify(() => findChild(title, "explicitBadge") === null)
    }
    function test_generic_select_retains_geometry_keys_and_marks() {
        const text = findChild(ordinary, "advisoryText")
        verify(text.width > 0 && text.height > 0)
        verify(text.mapToItem(ordinary, 0, 0).y >= 0)
        verify(text.mapToItem(ordinary, 0, text.height).y <= ordinary.height)
        ordinary.forceActiveFocus(); keyClick(Qt.Key_Down)
        compare(ordinary.value, "b")
        const menu = findChild(ordinary, "selectOptions")
        compare(menu.entries[0].mark, "BP")
        compare(menu.entries[1].mark, "E")
        compare(menu.entries[1].explicitContent, false)
    }
}
