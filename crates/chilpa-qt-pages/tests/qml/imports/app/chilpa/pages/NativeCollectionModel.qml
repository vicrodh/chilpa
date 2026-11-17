import QtQuick
import app.chilpa.sources
import app.chilpa.pages

QtObject {
    id:model
    property bool active:false
    property bool busy:false
    property bool simulateAsync:false
    property int count:0
    property int revision:0
    property string document:"{}"
    property string action:"{}"
    property string key:""
    property var calls:[]
    property var originals:[]
    property var held:({})
    property var resolved:({})
    property int closes:0
    property int released:0
    property string preview:"{}"
    property int previewsReleased:0
    property int syntheticCount:0
    function open(route) {
        if (!active) return false
        if (key === route) return true
        key=route;calls=calls.concat([["open",route]])
        const value=JSON.parse(Sources.page(route,0) || "{}")
        originals=value.items || []
        const header=Object.assign({},value);delete header.items
        count=value.knownCount || originals.length
        document=JSON.stringify({header:header,knownCount:count,hasMore:!!value.next_key,generation:1,anchorIndex:value.anchorIndex || 0})
        held={};seek(0,Math.min(99,count-1));return true
    }
    function seek(first,last) {
        if (!active || first < 0 || first >= count) return
        calls=calls.concat([["seek",first,last]])
        const next={}
        for (let i=first;i<=Math.min(last,first+99,count-1);++i) next[i]=Object.assign({},originals[i] || {kind:"track",title:"Song "+i},{rowToken:"row:"+i})
        held=next;revision++
    }
    function entry(index,rev) { return JSON.stringify(held[index] || {}) }
    function refresh(manual) { calls=calls.concat([["refresh",manual]]);NativePages.revalidate(key,manual);if(simulateAsync)busy=true }
    function prepareAction(token) {
        releaseAction()
        calls=calls.concat([["action",token]])
        const at=Number(token.split(":")[1]),item=originals[at] || {kind:"track",title:"Song "+at}
        const result=resolved[token] || {lease:1,key:"fixture frozen:"+Math.floor(at/100)+"|tracks",kind:item.kind,trackRow:item._host?.track_row ?? at%100,cardId:item._host?.card_id || 0,navigationKey:"fixture native:"+item.ref,ref:item.ref,title:item.title,artist:item.artist,available:item.available}
        action=JSON.stringify(result);return true
    }
    function pinAction() {
        if (!active || !JSON.parse(action).lease) return false
        preview=action;calls=calls.concat([["pin",JSON.parse(action).ref]]);return true
    }
    function releasePreview() { if(preview !== "{}")previewsReleased++;preview="{}" }
    function releaseAction() { if (action !== "{}") released++;action="{}" }
    function retire() { closes++;key="";busy=false;releaseAction();releasePreview() }
    function close() { retire();held={};count=0;document="{}" }
    function poll() {}
}
