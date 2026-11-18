import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "qrc:/qt/qml/app/chilpa/sources/qml/Screens"

TestCase {
    id: test
    name: "SourceSubtabs"
    when: windowShown
    visible: true
    width: 600; height: 800

    Component { id: pageFactory; SourceHome { width: test.width; height: test.height } }
    readonly property var tabs: [{title:"Home",ref:"discover:home",key:"fixture native:discover:home"},
        {title:"Styles",ref:"discover:styles",key:"fixture native:discover:styles",active:true}]
    function subtabs(active) {
        return Array.from({length:24}, (_, index) => ({title:"Style " + index, ref:"discover:styles:" + index,
            key:"fixture native:discover:styles:" + index, active:index === active}))
    }
    function open(key, document) {
        Sources.pages = {[key]: JSON.stringify(document)}
        const page = createTemporaryObject(pageFactory, test, {key:key})
        verify(page)
        tryVerify(() => page.headerItem !== null)
        return page
    }
    function emptyState(page) { return findChild(page.headerItem, "sourceEmptyState") }

    function test_a_tab_that_only_links_shows_its_line_without_an_empty_page() {
        const page = open("fixture native:discover:styles", {title:"Music",tabs:tabs,subtabs:subtabs(-1)})
        const line = findChild(page.headerItem, "sourceSubtabs")
        verify(line.visible)
        verify(!emptyState(page).visible, "a tab that only offers its pages must not say that nothing is here")
    }

    function test_a_discovery_tab_of_links_alone_shows_its_line_without_an_empty_page() {
        const page = open("fixture native:discover:styles", {title:"Music",tab:"styles",native_page:true,plugin:"fixture",reference:"discover:styles",
            loading:false,error:"",personal:false,genres:[],genre_filter:"",tabs:tabs,subtabs:subtabs(-1),shelves:[]})
        tryVerify(() => findChild(page.headerItem, "sourceSubtabs").height > 0)
        verify(!emptyState(page).visible, "a tab that only offers its pages must not say that nothing is here")
    }

    function test_a_tab_that_only_links_opens_its_line_in_place_without_leaving_the_page() {
        const pages = subtabs(-1)
        const menu = [{title:"Home",ref:"discover:home",key:"fixture native:discover:home",active:true},
            {title:"Styles",ref:"discover:styles",key:"fixture native:discover:styles",pages:pages}]
        const page = open("fixture native:discover:home", {title:"Music",tabs:menu,shelves:[]})
        const line = findChild(page.headerItem, "sourceSubtabs")
        verify(!line.visible)
        const pill = text => { let found = null; const walk = item => { if (item.text === text && item.clicked) found = item; for (const child of item.children || []) walk(child) }; walk(page.headerItem); return found }
        waitForRendering(page)
        const before = Navigation.went ? Navigation.went.length : 0
        mouseClick(pill("Styles"))
        tryVerify(() => line.visible && line.height > 0)
        compare(Navigation.went ? Navigation.went.length : 0, before, "opening the line must not leave the page")
        compare(pill("Styles").kind, "primary")
        mouseClick(pill("Style 3"))
        compare(Navigation.went[1], "fixture native:discover:styles:3")
        mouseClick(pill("Home"))
        tryVerify(() => !line.visible, 1000, "the tab the page is on closes the line")
    }

    function test_the_line_sits_under_the_tabs_and_wraps_inside_the_page() {
        const page = open("fixture native:discover:styles", {title:"Music",tabs:tabs,subtabs:subtabs(-1)})
        const line = findChild(page.headerItem, "sourceSubtabs")
        tryVerify(() => line.height > 0)
        let lowestTab = 0
        const walk = item => {
            if (item === line) return
            if (item.text === "Styles") lowestTab = Math.max(lowestTab, item.mapToItem(page.headerItem, 0, item.height).y)
            for (const child of item.children || []) walk(child)
        }
        walk(page.headerItem)
        verify(lowestTab > 0)
        verify(line.mapToItem(page.headerItem, 0, 0).y >= lowestTab, "the linked pages belong on their own line under the tabs")
        const pills = []
        for (const child of line.children) if (child.text !== undefined && child.text.startsWith("Style ")) pills.push(child)
        compare(pills.length, 24)
        verify(pills.every(pill => pill.x + pill.width <= line.width + 0.5), "every linked page must stay inside the page width")
        verify(pills[23].y > pills[0].y, "what does not fit on one line continues on the next")
    }

    function test_choosing_a_linked_page_opens_it_and_an_empty_one_says_so() {
        const page = open("fixture native:discover:styles", {title:"Music",tabs:tabs,subtabs:subtabs(-1)})
        const line = findChild(page.headerItem, "sourceSubtabs")
        let third = null
        for (const child of line.children) if (child.text === "Style 3") third = child
        verify(third)
        waitForRendering(page)
        mouseClick(third)
        compare(Navigation.went[1], "fixture native:discover:styles:3")
        const chosen = open("fixture native:discover:styles:3", {title:"Music",tabs:tabs,subtabs:subtabs(3)})
        verify(emptyState(chosen).visible, "a chosen page with no content still reports it")
    }

    function test_pages_without_linked_pages_keep_no_line() {
        const page = open("fixture native:discover:home", {title:"Music",tabs:tabs})
        verify(!findChild(page.headerItem, "sourceSubtabs").visible)
        verify(emptyState(page).visible)
    }
}
