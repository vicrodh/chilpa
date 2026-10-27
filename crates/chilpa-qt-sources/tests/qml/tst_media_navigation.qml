import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import app.chilpa.devices
import "../../qml/Screens"

TestCase {
    name: "MediaNavigation"
    when: windowShown
    visible: true
    width: 1000; height: 700
    MediaSourceScreen { id: page; anchors.fill: parent }
    MediaDetailScreen { id: detail; anchors.fill: parent; visible: false }
    function init() {
        Sources.asked = []
        Sources.show({source:"fixture",medium:"podcasts",media_tab:"home",crumbs:[],entries:[],columns:[]})
    }
    function test_episode_opens_its_parent_instead_of_history() {
        Sources.show({detail: {id: "episode", source: "fixture", title: "Episode", parent: {id: "opaque-show", title: "Podcast"}}})
        detail.visible = true
        const parent = findChild(detail, "episodeParent")
        compare(parent.text, "Podcast")
        parent.clicked()
        compare(Sources.asked[Sources.asked.length - 1], ["parent", "episode", "fixture"])
        compare(Navigation.went, ["source", "fixture"])
        Sources.show({detail: {id: "radio", live: true}})
        verify(!parent.visible)
        detail.visible = false
    }
    function test_only_explicit_podcast_audio_offers_device_sync() {
        Sources.show({detail: {id:"episode",source:"fixture",title:"Episode",device_sync:true}})
        detail.visible = true
        const button = findChild(detail, "episodeDevice")
        verify(button.visible)
        button.clicked()
        compare(DeviceSync.lastAction, ["episode","fixture","episode"])
        compare(Navigation.page,"deviceSync")
        Sources.show({detail:{id:"music",source:"fixture",device_sync:false}})
        verify(!button.visible)
        detail.visible = false
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
