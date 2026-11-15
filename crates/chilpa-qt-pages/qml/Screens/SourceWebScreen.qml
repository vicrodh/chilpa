import QtQuick
import QtQuick.Controls.Basic
import app.chilpa
import app.chilpa.kit
import app.chilpa.sources
import app.chilpa.pages
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "qrc:/qt/qml/app/chilpa/kit/qml/Primitives"

Item {
    id: page
    required property string source
    property bool leaving: false
    property string document: "{}"
    function refreshDocument() {
        if (visible && !leaving) document = Sources.page(source, Library.remoteRev + Sources.revision) || "{}"
    }
    onSourceChanged: refreshDocument()
    onVisibleChanged: refreshDocument()
    onLeavingChanged: refreshDocument()
    Connections { target: Library; function onRemoteRevChanged() { page.refreshDocument() } }
    Connections { target: Sources; function onRevisionChanged() { page.refreshDocument() } }
    readonly property var about: JSON.parse(document)
    readonly property string address: about.web?.url || ""
    readonly property string pluginId: about.plugin || ""
    readonly property bool permitted: address !== "" && NativePages.webAllowed(pluginId,address,address,Sources.revision + Library.remoteRev + Prefs.rev)
    readonly property bool activePage: visible && !leaving && permitted
    function updateBrowser() {
        if (!activePage) { browser.active=false; browser.source=""; return }
        if (!browser.active || String(browser.source) === "") {
            browser.setSource("qrc:/qt/qml/app/chilpa/pages/qml/EmbeddedWebPage.qml", {pluginId:pluginId,initialUrl:address})
            browser.active=true
        }
    }
    onActivePageChanged:updateBrowser()
    onAddressChanged: { browser.active=false;browser.source="";updateBrowser() }
    Component.onCompleted: { refreshDocument(); updateBrowser() }
    Component.onDestruction: { browser.active=false;browser.source="" }
    Column {
        id: toolbar
        x:Metrics.contentX;y:Metrics.contentTop
        width:parent.width-Metrics.contentX*2 // derived: padding on both sides
        spacing:Metrics.gap12
        Text { text:page.about.title||"";textFormat:Text.PlainText;font.pointSize:Type.title;font.weight:Type.bold;color:Theme.color.textPrimary }
        Row {
            spacing:Metrics.gap8
            IconButton { tip:I18n.tr("Back",I18n.rev);glyph:Icons.chevronLeft;enabled:browser.item?.canGoBack===true;onClicked:browser.item.goBack() }
            IconButton { tip:I18n.tr("Forward",I18n.rev);glyph:Icons.chevronRight;enabled:browser.item?.canGoForward===true;onClicked:browser.item.goForward() }
            IconButton { tip:I18n.tr("Reload",I18n.rev);glyph:Icons.refresh;enabled:!!browser.item;onClicked:browser.item.reload() }
            PillButton { text:I18n.tr("Open in browser",I18n.rev);enabled:page.address!=="";onClicked:Qt.openUrlExternally(browser.item?.address || page.address) }
            BusyIndicator { width:Metrics.button28;height:width;visible:page.about.loading===true||browser.item?.loading===true;running:visible&&page.visible&&!page.leaving }
        }
    }
    Loader {
        id:browser
        objectName:"embeddedPageLoader"
        anchors.left:parent.left;anchors.right:parent.right
        anchors.top:toolbar.bottom;anchors.topMargin:Metrics.gap16;anchors.bottom:parent.bottom
        active:false
        asynchronous:true
    }
    Text {
        anchors.top:toolbar.bottom;anchors.topMargin:Metrics.gap16;anchors.horizontalCenter:parent.horizontalCenter
        width:parent.width-Metrics.contentX*2 // derived: padding on both sides
        visible:(!page.permitted&&page.about.loading!==true)||!!page.about.error||!!browser.item?.error||browser.status===Loader.Error
        text:page.about.error||browser.item?.error||(browser.status===Loader.Error?I18n.tr("The embedded browser could not be loaded. Open this page in your browser.",I18n.rev):"")||I18n.tr("This page is unavailable while offline or the source is disabled.",I18n.rev)
        textFormat:Text.PlainText;wrapMode:Text.Wrap;color:Theme.color.textSecondary
    }
}
