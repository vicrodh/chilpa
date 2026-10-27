import QtQuick
import app.chilpa.kit
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Patterns"
import "../Theme"

Item {
    id: gallery
    implicitHeight: samples.implicitHeight
    readonly property var document: ({has_profiles: true, profile_names: ["Player SD", "Reader card"], profile_values: ["demo", "offline"], profile_enabled:[true,false], profiles:[{id:"demo",name:"Player SD",connected:true},{id:"offline",name:"Reader card",connected:false}], destination_names: ["Player · Memory card"], destination_values: ["storage"], destination_details: ["40 GiB available"], destination_enabled: [true], selected: "2 selections", can_review: true, change_count: 3, page_count: 2})
    readonly property var review: ({phase: "review", label: "Ready to sync", profile: "demo", destination: "storage", plan: "demo-plan", summary: "2 to copy · 0 to update · 1 to remove · 20 unchanged", required: "80 MiB", copies:2, removals: 1})
    DeviceProfiles { id: profiles; rows: gallery.document.profiles; onForgetAsked: id => rows = rows.filter(row => row.id !== id) }
    Column {
        id: samples
        width: parent.width
        spacing: Metrics.gap16
        DeviceHeader { width: parent.width; document: gallery.document; status: gallery.review; onProfilesAsked: profiles.open() }
        DeviceSelection { width: parent.width; height: DeviceLayout.galleryHeight; rows: [{id:"one",name:"Evening music",description:"12 tracks",checked:true},{id:"two",name:"A saved query",description:"genre:Jazz year:1960..1969",checked:false}] }
        DeviceSelection { width: parent.width; height: DeviceLayout.galleryHeight; busy: true }
        DeviceSelection { width: parent.width; height: DeviceLayout.galleryHeight; kind:"queries"; page:1; pages:3; rows:[{id:"query-one",name:"A saved query",description:"genre:Jazz",checked:true}] }
        DevicePager { page:0; pages:3 }
        DevicePager { page:1; pages:3; busy:true }
        DevicePager { page:2; pages:3 }
        DeviceMusicRow { width: parent.width; entry: ({title:"An album",detail:"Artist · 12 tracks",kind:"album",open:true}) }
        DeviceMusicRow { width: parent.width; checkable:true; entry: ({title:"A selected album",detail:"Artist · 12 tracks",checked:true,open:true}) }
        DeviceMusicRow { width: parent.width; entry: ({title:"First song",detail:"Artist · Disc 1",number:1,kind:"track",open:false}) }
        DeviceMusicRow { width: parent.width; entry: ({title:"An artist",detail:"12 albums",kind:"artist",open:true}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; document: ({source:"recorded",title:"Player SD",recorded:true,view:"artists",rows:[{id:"artist:0",title:"An artist",detail:"12 albums",kind:"artist",open:true}],page:0,pages:1}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; document: ({source:"recorded",title:"Player SD",recorded:true,view:"playlists",rows:[{id:"playlist:0",title:"Evening music",path:"Playlists/Evening music.m3u8",kind:"playlist",open:true}],page:0,pages:1}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; document: ({source:"demo",title:"Evening music",can_back:true,rows:[{id:"album:0",title:"An album",detail:"Artist · 12 tracks",kind:"album",open:true}],page:0,pages:1}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; document: ({source:"demo",title:"Evening music",album:"An album",can_back:true,rows:[{id:"track:0",title:"First song",detail:"Artist · Disc 1",number:1,kind:"track",open:false}],page:0,pages:2}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; document: ({source:"recorded",title:"Player SD",recorded:true,notice:"Last synced with Chilpa. Files changed elsewhere may differ.",rows:[],page:0,pages:1}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; document: ({source:"demo",title:"Evening music",query:"Missing song",rows:[],page:0,pages:1}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; busy:true; document: ({source:"demo",title:"Evening music",loading:true,rows:[],page:0,pages:1}) }
        DeviceBrowser { width: parent.width; height: DeviceLayout.galleryHeight; document: ({source:"recorded",title:"Player SD",recorded:true,error:{message:"I could not read the last synced music for this storage.",detail:"The saved history is unavailable.",recovery:false},rows:[],page:0,pages:1}) }
        DeviceChanges { width: parent.width; height: DeviceLayout.galleryHeight; pages: 2; rows: [{action:"Copy",title:"First song",artist:"Artist",album:"Album",size:"24 MiB"},{action:"Remove",title:"Another song",artist:"Artist",album:"Album",size:"16 MiB"}] }
        DeviceChanges { width: parent.width; height: DeviceLayout.galleryHeight }
        DeviceNotice { width: parent.width; error: ({message:"The player is not responding. Reconnect it and try again.",detail:"A transport stopped responding.",recovery:false}) }
        DeviceNotice { width: parent.width; error: ({message:"An interrupted sync needs recovery before reviewing new changes.",detail:"",recovery:true}) }
        DeviceFooter { width: parent.width; document: gallery.document; status: gallery.review }
        DeviceFooter { width: parent.width; document: gallery.document; status: ({busy:true,phase:"transferring",label:"Syncing music…",total:10,completed:4,progress:0.4,progress_text:"4 of 10 files",current:"First song"}) }
        DeviceFooter { width: parent.width; document: gallery.document; status: ({phase:"error",label:"Sync needs attention",error:{recovery:true},total:10,progress:0.4,progress_text:"4 of 10 files"}) }
        DevicePairing { width: parent.width; height: DeviceLayout.galleryHeight }
    }
}
