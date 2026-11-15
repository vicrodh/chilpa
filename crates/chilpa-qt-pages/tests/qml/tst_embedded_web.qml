import QtQuick
import QtTest
import app.chilpa.pages

TestCase {
    id:test
    name:"EmbeddedWeb"
    when:windowShown
    width:800;height:600
    visible:true
    QtObject { id:profileWatch; property QtObject held:null }
    Loader { id:browser; anchors.fill:parent; active:false }
    function cleanup() { browser.active=false;browser.source="" }
    function test_private_profile_and_explicit_destruction() {
        NativePages.allowWeb=true
        browser.setSource("qrc:/qt/qml/app/chilpa/pages/qml/EmbeddedWebPage.qml",{pluginId:"fixture",initialUrl:"about:blank"})
        browser.active=true
        tryCompare(browser,"status",Loader.Ready)
        const view=findChild(browser.item,"embeddedWebView")
        verify(view)
        profileWatch.held=view.profile
        compare(view.profile.offTheRecord,true)
        compare(view.profile.httpCacheMaximumSize,8*1024*1024)
        compare(view.audioMuted,true)
        let title=""
        view.loadHtml("<!doctype html><title>Journal fixture</title><h1>Journal</h1>","https://example.test/journal")
        tryCompare(view,"title","Journal fixture")
        tryCompare(view,"loading",false)
        view.runJavaScript("document.title",result=>title=result)
        tryVerify(()=>title==="Journal fixture")
        const address=String(view.url)
        browser.item.openWindow({userInitiated:false,requestedUrl:"https://example.test/story"})
        compare(String(view.url),address)
        NativePages.allowWeb=false
        browser.item.openWindow({userInitiated:true,requestedUrl:"https://example.test/story"})
        compare(String(view.url),address)
        browser.active=false;browser.source=""
        tryCompare(browser,"item",null)
        tryCompare(profileWatch,"held",null)
    }
}
