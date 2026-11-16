import QtQuick
import app.chilpa.sources

QtObject {
    property bool active:false
    property bool busy:false
    property string document:"{}"
    property string error:""
    property int imageRevision:0
    property int serial:0
    property int openCount:0
    property int pollCount:0
    property int retireCount:0
    property var leases:({})
    property var watched:[]
    function open(key) {
        if(!active)return false
        openCount++
        document=Sources.pages[key] || "{}"
        error=""
        return true
    }
    function poll() { if(active)pollCount++ }
    function retire() { retireCount++;leases={};busy=false }
    function close() { retire();document="{}";error="" }
    function watchImage(url) {
        if(!active || Object.keys(leases).length>=8)return 0
        const ticket=++serial
        leases=Object.assign({},leases,{[ticket]:url})
        watched=watched.concat([url])
        return ticket
    }
    function releaseImage(ticket) {
        const next=Object.assign({},leases)
        delete next[ticket]
        leases=next
    }
    function image(url,rev) { return "" }
    function invalidate() { close();error="The source changed." }
}
