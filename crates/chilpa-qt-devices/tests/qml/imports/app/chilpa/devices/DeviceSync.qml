pragma Singleton
import QtQuick

QtObject {
    property var status: ({phase:"idle",busy:false,profile:"profile",destination:"storage",label:"Choose music for this storage",remove_unselected:false,plan:"",removals:0,error:null})
    property var data: ({has_profiles:true,profile_folder:"Chilpa",storage_detail:"40 GiB available of 64 GiB",profile_names:["Player SD"],profile_values:["profile"],destination_names:["Player · Memory card"],destination_values:["storage"],destination_enabled:[true],destination_details:["40 GiB available"],options:[],selected:"1 selection",can_review:true,change_count:3,page_count:2})
    property string snapshot: JSON.stringify(status)
    property string catalog: JSON.stringify(data)
    property string pageRows: '[{"path":"Music/a.flac","action":"Copy","title":"A song","artist":"Artist","album":"Album","size":"24 MiB"}]'
    property var music: ({})
    property string browser: JSON.stringify(music)
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
    function browse(source) {
        lastAction = ["browse",source]
        music = {source:source,title:source === "recorded" ? "Player SD" : "Evening music",recorded:source === "recorded",notice:source === "recorded" ? "Last synced with Chilpa. Files changed elsewhere may differ." : "",can_back:source !== "recorded",level:"albums",rows:[{id:"album:0",title:"Record",detail:"Artist · 2 tracks",kind:"album",open:true}],page:0,pages:1}
    }
    function openBrowserAlbum(id) {
        lastAction = ["album",id]
        music = Object.assign({},music,{album:"Record",can_back:true,level:"tracks",rows:[{id:"track:0",title:"First song",detail:"Artist · Disc 1",kind:"track",open:false},{id:"track:1",title:"Second song",detail:"Artist · Disc 2",kind:"track",open:false}]})
    }
    function browserBack() { lastAction = ["browserBack"]; if (music.album) music = Object.assign({},music,{album:null,level:"albums",rows:[{id:"album:0",title:"Record",detail:"Artist · 2 tracks",kind:"album",open:true}],can_back:!music.recorded}); else music = ({}) }
    function filterBrowser(text) { lastAction = ["browserFilter",text] }
    function browserPage(at) { lastAction = ["browserPage",at] }
    function closeBrowser() { music = ({}) }
    function setStatus(value) { status = Object.assign({},status,value) }
}
