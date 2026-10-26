import QtQuick
import "qrc:/qt/qml/app/chilpa/kit/qml/Theme"
import "../Components"
import "../Patterns"
import "../Theme"

Item {
    id: gallery
    implicitHeight: samples.implicitHeight
    readonly property var document: ({has_profiles: true, profile_names: ["Player SD"], profile_values: ["demo"], destination_names: ["Player · Memory card"], destination_values: ["storage"], destination_details: ["40 GiB available"], destination_enabled: [true], selected: "2 selections", can_review: true, change_count: 3, page_count: 2})
    readonly property var review: ({phase: "review", label: "Ready to sync", profile: "demo", destination: "storage", plan: "demo-plan", summary: "2 to copy · 0 to update · 1 to remove · 20 unchanged", required: "80 MiB", removals: 1})
    Column {
        id: samples
        width: parent.width
        spacing: Metrics.gap16
        DeviceHeader { width: parent.width; document: gallery.document; status: gallery.review }
        DeviceSelection { width: parent.width; height: DeviceLayout.galleryHeight; rows: [{id:"one",name:"Evening music",description:"12 tracks",checked:true},{id:"two",name:"A saved query",description:"genre:Jazz year:1960..1969",checked:false}] }
        DeviceSelection { width: parent.width; height: DeviceLayout.galleryHeight; busy: true }
        DeviceMusicRow { width: parent.width; entry: ({title:"An album",detail:"Artist · 12 tracks",kind:"album",open:true}) }
        DeviceMusicRow { width: parent.width; checkable:true; entry: ({title:"A selected album",detail:"Artist · 12 tracks",checked:true,open:true}) }
        DeviceMusicRow { width: parent.width; entry: ({title:"First song",detail:"Artist · Disc 1",number:1,kind:"track",open:false}) }
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
