pragma Singleton
import QtQuick

QtObject {
    property var status: ({phase:"idle",busy:false,profile:"profile",destination:"storage",label:"Choose music for this storage",remove_unselected:false,plan:"",removals:0,error:null})
    property var data: ({has_profiles:true,profile_folder:"Chilpa",storage_detail:"40 GiB available of 64 GiB",profile_names:["Player SD"],profile_values:["profile"],destination_names:["Player · Memory card"],destination_values:["storage"],destination_enabled:[true],destination_details:["40 GiB available"],options:[],selected:"1 selection",can_review:true,change_count:3,page_count:2})
    property string snapshot: JSON.stringify(status)
    property string catalog: JSON.stringify(data)
    property string pageRows: '[{"path":"Music/a.flac","action":"Copy","title":"A song","artist":"Artist","album":"Album","size":"24 MiB"}]'
    readonly property bool busy: status.busy
    property int page: 0
    property bool hasConnected: true
    property int polls: 0
    property int enters: 0
    property int leaves: 0
    property var lastAction: []
    signal profileCreated()
    function enter() { enters++ }
    function leave() { leaves++ }
    function poll() { polls++ }
    function refresh() { lastAction = ["refresh"] }
    function selectProfile(id) { lastAction = ["profile",id] }
    function selectDestination(id) { lastAction = ["destination",id] }
    function createProfile(name,folder) { lastAction = ["create",name,folder] }
    function toggle(id) { lastAction = ["toggle",id] }
    function useTracks(ids) { lastAction = ["tracks",ids] }
    function filter(text) { lastAction = ["filter",text] }
    function removals(enabled) { lastAction = ["removals",enabled] }
    function preview(recover) { lastAction = ["preview",recover] }
    function apply(token,removals) { lastAction = ["apply",token,removals] }
    function cancel() { lastAction = ["cancel"] }
    function changePage(at) { page = at; lastAction = ["page",at] }
    function setStatus(value) { status = Object.assign({},status,value) }
}
