import QtQuick
import QtTest
import com.blitzfc.chilpa.sources
import "../../qml/Screens"

TestCase {
    name: "MediaNavigation"
    when: windowShown
    visible: true
    width: 1000; height: 700
    MediaSourceScreen { id: page; anchors.fill: parent }
    function init() {
        Sources.asked = []
        Sources.show({source:"fixture",medium:"podcasts",media_tab:"home",crumbs:[],entries:[],columns:[]})
    }
    function test_sidebar_destination_and_page_tabs_share_confirmed_state() {
        page.go("categories")
        compare(Sources.asked[0],["child","fixture","media:categories"])
        compare(page.tab,"home")
        Sources.show({media_tab:"categories"})
        compare(page.tab,"categories")
        verify(page.indexPage)
        Sources.show({medium:"radio",media_tab:"countries"})
        verify(page.radio)
        compare(page.tab,"countries")
        verify(page.indexPage)
        page.go("saved")
        compare(Sources.asked[1],["child","fixture","media:saved"])
    }
}
