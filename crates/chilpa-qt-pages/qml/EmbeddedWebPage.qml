import QtQuick
import QtWebEngine
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.pages

Item {
    id:browser
    required property string pluginId
    required property url initialUrl
    readonly property url address:view.url
    readonly property bool loading:view.loading
    readonly property bool canGoBack:view.canGoBack
    readonly property bool canGoForward:view.canGoForward
    property string error:""
    function openWindow(request) {
        if(request.userInitiated && NativePages.webAllowed(pluginId,String(initialUrl),String(request.requestedUrl),Sources.revision+Library.remoteRev)) view.url=request.requestedUrl
    }
    function goBack(){view.goBack()}
    function goForward(){view.goForward()}
    function reload(){error="";view.reload()}
    Component.onDestruction:view.stop()
    WebEngineView {
        id:view
        objectName:"embeddedWebView"
        anchors.fill:parent
        url:browser.initialUrl
        audioMuted:true
        profile:WebEngineProfile {
            offTheRecord:true
            httpCacheType:WebEngineProfile.MemoryHttpCache
            httpCacheMaximumSize:8*1024*1024
            persistentCookiesPolicy:WebEngineProfile.NoPersistentCookies
            persistentPermissionsPolicy:WebEngineProfile.AskEveryTime
            isPushServiceEnabled:false
            onDownloadRequested:download=>download.cancel()
        }
        settings.playbackRequiresUserGesture:true
        settings.javascriptCanOpenWindows:false
        settings.pluginsEnabled:false
        settings.fullScreenSupportEnabled:false
        settings.screenCaptureEnabled:false
        settings.webRTCPublicInterfacesOnly:true
        settings.localContentCanAccessFileUrls:false
        settings.localContentCanAccessRemoteUrls:false
        onPermissionRequested:permission=>permission.deny()
        onFullScreenRequested:request=>request.reject()
        onNavigationRequested:request=>{
            if(!NativePages.webAllowed(browser.pluginId,String(browser.initialUrl),String(request.url),Sources.revision+Library.remoteRev))request.reject()
        }
        onNewWindowRequested:request=>browser.openWindow(request)
        onLoadingChanged:request=>{
            if(request.status===WebEngineView.LoadFailedStatus)browser.error=request.errorString||"The page could not be loaded."
            else if(request.status===WebEngineView.LoadStartedStatus||request.status===WebEngineView.LoadSucceededStatus)browser.error=""
        }
    }
}
