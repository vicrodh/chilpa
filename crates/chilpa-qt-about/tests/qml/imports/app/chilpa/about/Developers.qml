pragma Singleton
import QtQuick
QtObject {
    property string diagnostics: ""
    property string logs: ""
    property string review: ""
    property string status: ""
    property string uploadedUrl: ""
    property bool uploading: false
    property int bundled: 0
    property int allCopied: 0
    property int uploads: 0
    property int urlCopied: 0
    property int refreshed: 0
    property int logsRefreshed: 0
    property int copied: 0
    property int saved: 0
    function refresh() { refreshed++; diagnostics=JSON.stringify({sections:[{title:"System",rows:[{label:"Fixture",runtime:"Observed"}]}]}) }
    function refreshLogs(level,search) {
        logsRefreshed++
        const rows=[]
        for (let i=0;i<512;i++) rows.push({ts:1234,level:"stderr",target:"fixture",message:"Redacted fixture "+i})
        logs=JSON.stringify({rows:rows,total:512,shown:512,fileAvailable:false})
    }
    function clearLogs() { logs='{"rows":[],"total":0,"shown":0}' }
    function release() { diagnostics=""; releaseLogs(); closeReport() }
    function releaseLogs() { logs="" }
    function prepareReport() { review="Reviewed fixture report\n[redacted sensitive log entry]"; status="" }
    function closeReport() { review=""; status="" }
    function copyReport() { copied++; status="Copied" }
    function exportReport() { saved++; status="Report saved" }
    function copyLogs(level,search) { allCopied++; status="Copied" }
    function copyBundle() { bundled++; status="Copied" }
    function uploadBundle() { uploads++; uploadedUrl="https://paste.invalid/abc" }
    function copyUrl() { urlCopied++; status="Copied" }
    function logUrl() { return "" }
}
