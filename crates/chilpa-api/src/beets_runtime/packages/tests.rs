use super::*;
fn archive(name: &str, contents: &[u8]) -> Vec<u8> {
    let mut output = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut output);
        writer.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(contents).unwrap(); writer.finish().unwrap();
    }
    output.into_inner()
}
#[test]
fn bootstrap_platforms_and_interpreter_version_are_pinned() {
    let catalog = catalog();
    assert_eq!((catalog.uv.as_str(),catalog.python.as_str()),("0.13.8",PYTHON));
    assert_eq!(catalog.platforms.len(),6);
    for artifact in &catalog.platforms {
        assert_eq!(url::Url::parse(&artifact.url).unwrap().host_str(),Some("files.pythonhosted.org"));
        assert!(artifact.url.starts_with("https://"));
        assert_eq!(artifact.sha256.len(),64);
        assert!(artifact.bytes<MAX_BOOTSTRAP);
        crate::plugin_package::portable_path(&artifact.program).unwrap();
    }
}
#[test]
fn bootstrap_archives_cannot_replace_a_file_outside_staging() {
    let root = tempfile::tempdir().unwrap();
    let sentinel = root.path().join("untouched"); std::fs::write(&sentinel,b"original").unwrap();
    let mut item = catalog().platforms[0].clone(); item.program="package/uv".into();
    for name in ["../untouched","/untouched","package/../../untouched"] {
        assert!(matches!(unpack(root.path(),&item,&archive(name,b"replacement")),Err(Failure::Archive)));
        assert_eq!(std::fs::read(&sentinel).unwrap(),b"original");
    }
    let package = unpack(root.path(),&item,&archive("package/uv",b"fixture executable")).unwrap();
    assert_eq!(std::fs::read(&package.executable).unwrap(),b"fixture executable");
}
#[test]
fn only_marked_app_generations_are_eligible_for_cleanup() {
    let root = tempfile::tempdir().unwrap();
    for name in ["managed-one","personal"] { fs::create_dir(root.path().join(name)).unwrap(); }
    assert!(!owned(root.path(),"managed-one"));
    fs::write(root.path().join("managed-one/owner"),OWNER).unwrap();
    fs::write(root.path().join("personal/owner"),OWNER).unwrap();
    assert!(owned(root.path(),"managed-one"));
    assert!(!owned(root.path(),"personal"));
    assert!(!owned(root.path(),"../managed-one"));
}
#[test]
fn an_oversized_environment_is_rejected_without_reading_its_contents() {
    let root = tempfile::tempdir().unwrap();
    fs::File::create(root.path().join("oversized")).unwrap().set_len(MAX_RUNTIME+1).unwrap();
    assert_eq!(check_size(root.path()),Err(Failure::DiskLimit));
}
