pragma Singleton
import QtQuick

// The About bridge as the tests see it: the document a test writes.
QtObject {
    property string doc: JSON.stringify({ name: "Chilpa", version: "1.2.3", build: "2026-10-09 (abcdef123)", platform: "Linux (Qt)", author: { name: "vicrodh", url: "https://github.com/vicrodh", avatar: "vicrodh.png" }, contributors: ["afonsojramos", "jakumule", "TheMelmacian"].map(handle => ({name: handle, url: "https://github.com/" + handle, avatar: handle + ".png"})), links: [{ label: "Website", url: "https://example.org" }] })
    signal issueRequested()
    function reportIssue() { issueRequested() }
    function document(rev) { return doc }
    function link(name) { return name === "documentation" || name === "issue" ? "https://example.org/" + name : "" }
}
