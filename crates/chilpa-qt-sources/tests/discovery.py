#!/usr/bin/env python3
"""I exercise the production native page components with isolated preferences."""
import pathlib
import shutil
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[3]
with tempfile.TemporaryDirectory(prefix="chilpa-discovery-") as directory:
    stage = pathlib.Path(directory)
    shutil.copytree(root / "crates/chilpa-qt/tests/qml/imports", stage / "imports")
    mock = stage / "imports/app/chilpa/sources/Sources.qml"
    mock.write_text(mock.read_text().replace('return "{}"', 'return JSON.stringify({plugin:"fixture",kind:"album",ref:"album:1",title:"Record",artist_ref:"artist:7",label_ref:"label:9",favorite:false})'))
    mock.write_text(mock.read_text().replace('function artistKey(id, path) { return "" }', 'function artistKey(id, path) { return "fixture native:artist:7" }'))
    module = stage / "imports/app/chilpa"
    (module / "AlbumActions.qml").write_text('pragma Singleton\nimport QtQuick\nQtObject { property int rev: 0; property int selectedCount: 0; function selected(id, rev) { return false } }\n')
    with (module / "qmldir").open("a") as stream: stream.write("\nsingleton AlbumActions 1.0 AlbumActions.qml\n")
    for package, target in [("chilpa-qt", "host"),("chilpa-qt-sources", "sources")]:
        shutil.copytree(root / "crates" / package / "qml", stage / target)
    shutil.copytree(root / "crates/chilpa-qt-kit/assets", stage / "assets")
    for path in stage.rglob("*.qml"):
        text = path.read_text().replace("qrc:/qt/qml/app/chilpa/sources/qml/", (stage / "sources").as_uri()+"/").replace("qrc:/qt/qml/app/chilpa/qml/", (stage / "host").as_uri()+"/")
        path.write_text(text)
    (stage / "tst_discovery.qml").write_text('''import QtQuick
import QtTest
import app.chilpa
import app.chilpa.sources
import "sources/Patterns"
import "host/Components"
TestCase {
    name: "NativeDiscovery"; when: windowShown; visible: true; width: 1100; height: 800
    AlbumMenu { id: albumMenu }
    SourceDiscoveryControls { id: controls; width: parent.width; about: ({plugin:"fixture",tab:"home",shelves:[{id:"a",title:"A",enabled:true},{id:"b",title:"B",enabled:false}],genres:[{id:1,name:"Genre"}]}) }
    SourceRecordDetails { id: details; width: parent.width; cardId: -1 }
    SourceShelf { width: parent.width; section: ({id:"empty",key:"fixture native:home|empty",title:"Empty",error:"Unavailable"}) }
    function findButton(item, text) {
        if (item.text === text && typeof item.clicked === "function") return item
        for (let child of item.children || []) { const found = findButton(child,text); if (found) return found }
        return null
    }
    function test_album_context_menu_uses_the_native_artist_and_favorites() {
        albumMenu.show(-1,"Record","Band",0,0,0)
        let index = albumMenu.actions.findIndex(a => a.act === "artist")
        verify(index >= 0)
        albumMenu.chosen(index)
        compare(Navigation.went,["source","fixture native:artist:7"])
        index = albumMenu.actions.findIndex(a => a.act === "favorite")
        verify(index >= 0)
        albumMenu.chosen(index)
        const sent = Sources.asked[Sources.asked.length-1]
        compare(sent[0],"native")
        compare(JSON.parse(sent[2]),{action:"favorite",ref:"album:1",on:true})
        albumMenu.close()
    }
    function test_native_context_links_and_favorites() {
        verify(details.visible)
        details.open(details.info.artist_ref)
        compare(Navigation.went, ["source","fixture native:artist:7"])
        details.open(details.info.label_ref)
        compare(Navigation.went, ["source","fixture native:label:9"])
        const button = findButton(details, "Add to favorites")
        verify(button !== null)
        button.clicked()
        const sent = Sources.asked[Sources.asked.length-1]
        compare(sent[0],"native"); compare(sent[1],"fixture")
        compare(JSON.parse(sent[2]),{action:"favorite",ref:"album:1",on:true})
    }
    function test_order_visibility_and_sizes_persist_per_tab() {
        compare(controls.shown.length, 1)
        controls.change(1,"enabled",true)
        controls.move(1,-1)
        controls.change(0,"size",6)
        compare(controls.shown[0].id,"b")
        compare(controls.shown[0].size,6)
        const home = controls.about
        controls.about = Object.assign({},home,{tab:"editors"})
        compare(controls.shown.length,1)
        compare(controls.shown[0].id,"a")
        controls.about = home
        compare(controls.shown[0].id,"b")
        compare(controls.shown[0].size,6)
    }
}
''')
    result = subprocess.run(["/usr/lib64/qt6/bin/qmltestrunner", "-platform", "offscreen", "-import", str(stage / "imports"), "-input", str(stage / "tst_discovery.qml")],check=False)
    raise SystemExit(result.returncode)
