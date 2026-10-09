pragma Singleton
import QtQuick

// The About bridge as the tests see it: the document a test writes.
QtObject {
    property string doc: JSON.stringify({ name: "Chilpa", version: "1.2.3", build: "2026-10-09 (abcdef123)", platform: "Linux (Qt)", links: [{ label: "Website", url: "https://example.org" }] })
    function document(rev) { return doc }
}
