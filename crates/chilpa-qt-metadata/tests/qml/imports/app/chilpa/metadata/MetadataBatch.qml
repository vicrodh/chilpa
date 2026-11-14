import QtQuick
QtObject {
    property string document: "{}"
    property string kind: "items"
    property string groupKey: ""
    property bool active: false
    property bool busy: false
    property int count: 0
    property int revision: 0
    property var requests: []
    property var rows: [{key:"1:1",path:"/music/01.flac",artist:"Avantasia",album:"Album",title:"Track",selected:true,changes:[]}]
    property int saves: 0
    property int polls: 0
    function publish(value) { document=JSON.stringify(value);count=value.counts?.files || 0;busy=!!value.busy;revision++ }
    function openPaths(paths) {
        requests=requests.concat([{action:"open",value:JSON.parse(paths)}])
        publish({session:1,state:"ready",busy:false,dirty:false,beets:{ready:true},counts:{files:1,groups:1,selected:1},summary:{changed:0},fields:[{key:"genres",label:"Genres",type:"list",operations:["keep","set","clear","append_unique","remove_values"]}]})
        return true
    }
    function select(value) { requests=requests.concat([{action:"select",value:JSON.parse(value)}]);return true }
    function patch(value) { requests=requests.concat([{action:"patch",value:JSON.parse(value)}]);return true }
    function save() { saves++;return true }
    function retry() { return true }
    function browse(section,group) { kind=section;groupKey=group;requests=requests.concat([{action:"browse",kind:section,group:group}]) }
    function lookupGroup(key) { requests=requests.concat([{action:"lookup",key:key}]);return true }
    function candidate(value) { requests=requests.concat([{action:"candidate",value:JSON.parse(value)}]);return true }
    function groupPlan(value) { requests=requests.concat([{action:"groupPlan",value:JSON.parse(value)}]);return true }
    function applyPlan(id) { requests=requests.concat([{action:"applyPlan",id:id}]);return true }
    function cancel() { busy=false }
    function close() { busy=false;document="{}";count=0 }
    function poll() { polls++ }
    function seek(first,last) { requests=requests.concat([{action:"seek",first:first,last:last}]).slice(-32) }
    function entry(index,revision) { return JSON.stringify(rows[index] || {}) }
}
