use super::*;

/// What a synthetic ELF file says about itself.
#[derive(Default)]
struct Fixture {
    class: Option<u8>,
    machine: Option<u16>,
    interpreter: Option<&'static str>,
    needed: Vec<&'static str>,
    runpath: Option<String>,
    wants: Vec<(&'static str, Vec<&'static str>)>,
    defines: Vec<&'static str>,
    /// More `DT_NEEDED` entries, each naming the first one's string again.
    again: usize,
}

fn add(strings: &mut Vec<u8>, text: &str) -> u32 {
    let at = strings.len() as u32;
    strings.extend(text.as_bytes());
    strings.push(0);
    at
}

/// A 64-bit little-endian ELF file laid out from address 0, so an address
/// is also its offset in the file: header, program headers, interpreter,
/// strings, version tables and the dynamic table.
fn elf(fixture: &Fixture) -> Vec<u8> {
    let mut strings = vec![0u8];
    let needed: Vec<u32> = fixture.needed.iter().map(|name| add(&mut strings, name)).collect();
    let runpath = fixture.runpath.as_deref().map(|path| add(&mut strings, path));
    let mut verneed = Vec::new();
    for (index, (file, versions)) in fixture.wants.iter().enumerate() {
        let file = add(&mut strings, file);
        let next = if index + 1 == fixture.wants.len() { 0 } else { 16 + 16 * versions.len() as u32 };
        for value in [1u16, versions.len() as u16] { verneed.extend(value.to_le_bytes()); }
        for value in [file, 16, next] { verneed.extend(value.to_le_bytes()); }
        for (position, version) in versions.iter().enumerate() {
            let name = add(&mut strings, version);
            verneed.extend(0u32.to_le_bytes());
            for value in [0u16, position as u16 + 2] { verneed.extend(value.to_le_bytes()); }
            let next = if position + 1 == versions.len() { 0u32 } else { 16 };
            for value in [name, next] { verneed.extend(value.to_le_bytes()); }
        }
    }
    let mut verdef = Vec::new();
    for (index, version) in fixture.defines.iter().enumerate() {
        let name = add(&mut strings, version);
        for value in [1u16, 0, index as u16 + 1, 1] { verdef.extend(value.to_le_bytes()); }
        let next = if index + 1 == fixture.defines.len() { 0u32 } else { 28 };
        for value in [0u32, 20, next, name, 0] { verdef.extend(value.to_le_bytes()); }
    }
    let headers = 2 + fixture.interpreter.is_some() as usize;
    let interpreter_at = 64 + 56 * headers;
    let interpreter = fixture.interpreter.map(|path| format!("{path}\0").into_bytes()).unwrap_or_default();
    let strings_at = interpreter_at + interpreter.len();
    let verneed_at = strings_at + strings.len();
    let verdef_at = verneed_at + verneed.len();
    let dynamic_at = (verdef_at + verdef.len()).next_multiple_of(8);
    let mut dynamic = Vec::new();
    let mut entry = |tag: u64, value: u64| { dynamic.extend(tag.to_le_bytes()); dynamic.extend(value.to_le_bytes()); };
    for name in &needed { entry(DT_NEEDED, *name as u64); }
    for _ in 0..fixture.again { entry(DT_NEEDED, needed[0] as u64); }
    if let Some(path) = runpath { entry(DT_RUNPATH, path as u64); }
    entry(DT_STRTAB, strings_at as u64);
    entry(DT_STRSZ, strings.len() as u64);
    if !fixture.wants.is_empty() { entry(DT_VERNEED, verneed_at as u64); entry(DT_VERNEEDNUM, fixture.wants.len() as u64); }
    if !fixture.defines.is_empty() { entry(DT_VERDEF, verdef_at as u64); entry(DT_VERDEFNUM, fixture.defines.len() as u64); }
    entry(0, 0);
    let total = dynamic_at + dynamic.len();

    let mut file = vec![0x7f, b'E', b'L', b'F', fixture.class.unwrap_or(2), 1, 1, 0];
    file.resize(16, 0);
    let machine = fixture.machine.unwrap_or_else(|| this_machine().map(|(machine, _)| machine).unwrap_or(62));
    for value in [3u16, machine] { file.extend(value.to_le_bytes()); }
    file.extend(1u32.to_le_bytes());
    for value in [0u64, 64, 0] { file.extend(value.to_le_bytes()); }
    file.extend(0u32.to_le_bytes());
    for value in [64u16, 56, headers as u16, 0, 0, 0] { file.extend(value.to_le_bytes()); }
    let mut header = |kind: u32, offset: usize, length: usize| {
        file.extend(kind.to_le_bytes());
        file.extend(4u32.to_le_bytes());
        for value in [offset, offset, offset, length, length, 8] { file.extend((value as u64).to_le_bytes()); }
    };
    header(PT_LOAD, 0, total);
    if fixture.interpreter.is_some() { header(PT_INTERP, interpreter_at, interpreter.len()); }
    header(PT_DYNAMIC, dynamic_at, dynamic.len());
    file.extend(interpreter);
    file.extend(strings);
    file.extend(verneed);
    file.extend(verdef);
    file.resize(dynamic_at, 0);
    file.extend(dynamic);
    file
}

/// A runtime with a loader and a glibc that defines versions up to 2.42.
fn system() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let triplet = this_machine().map(|(_, triplet)| triplet).unwrap_or("x86_64-linux-gnu");
    let libraries = root.path().join("usr/lib").join(triplet);
    std::fs::create_dir_all(&libraries).unwrap();
    std::fs::create_dir_all(root.path().join("lib64")).unwrap();
    std::fs::write(root.path().join("lib64/ld-test.so.2"), "loader").unwrap();
    std::fs::write(libraries.join("libc.so.6"), elf(&Fixture { defines: vec!["libc.so.6", "GLIBC_2.2.5", "GLIBC_2.5", "GLIBC_2.34", "GLIBC_2.42"], ..Default::default() })).unwrap();
    root
}

fn program(folder: &Path, fixture: &Fixture) -> PathBuf {
    std::fs::create_dir_all(folder).unwrap();
    let path = folder.join("plugin");
    std::fs::write(&path, elf(fixture)).unwrap();
    path
}

fn linked(wants: Vec<&'static str>) -> Fixture {
    Fixture { interpreter: Some("/lib64/ld-test.so.2"), needed: vec!["libc.so.6"], wants: vec![("libc.so.6", wants)], ..Default::default() }
}

#[test]
fn the_program_says_its_loader_libraries_and_newest_glibc() {
    let folder = tempfile::tempdir().unwrap();
    let path = program(folder.path(), &Fixture { needed: vec!["libssl.so.3", "libc.so.6"], runpath: Some("$ORIGIN/lib".into()),
        wants: vec![("libc.so.6", vec!["GLIBC_2.5", "GLIBC_2.34", "GLIBC_2.2.5"]), ("libssl.so.3", vec!["OPENSSL_3.0.0"])], ..linked(vec![]) });
    let Ok(Kind::Program(found)) = read(&path) else { panic!("{:?}", read(&path)) };
    assert_eq!(found.interpreter.as_deref(), Some("/lib64/ld-test.so.2"));
    assert_eq!(found.needed, vec!["libssl.so.3", "libc.so.6"]);
    assert_eq!(found.runpath, vec!["$ORIGIN/lib"]);
    assert_eq!(found.newest_glibc(), Some("GLIBC_2.34"), "2.34 is newer than 2.5, as numbers");
}

#[test]
fn a_program_whose_loader_libraries_and_versions_are_there_passes() {
    let system = system();
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(folder.path().join("lib")).unwrap();
    std::fs::write(folder.path().join("lib/libbundled.so.1"), elf(&Fixture::default())).unwrap();
    let path = program(folder.path(), &Fixture { needed: vec!["libbundled.so.1", "libc.so.6"], runpath: Some("$ORIGIN/lib".into()),
        ..linked(vec!["GLIBC_2.2.5", "GLIBC_2.34"]) });
    assert_eq!(preflight(&path, system.path()), Ok(()), "a library beside it, through $ORIGIN, counts");
}

#[test]
fn a_missing_library_or_loader_is_named() {
    let system = system();
    let folder = tempfile::tempdir().unwrap();
    let path = program(folder.path(), &Fixture { needed: vec!["libc.so.6", "libssl.so.3"], ..linked(vec!["GLIBC_2.34"]) });
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Library("libssl.so.3".into())));
    let path = program(folder.path(), &Fixture { interpreter: Some("/lib/ld-elsewhere.so.1"), ..linked(vec![]) });
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Interpreter("/lib/ld-elsewhere.so.1".into())));
}

#[test]
fn a_glibc_newer_than_the_systems_is_named_with_the_newest_one_there() {
    let system = system();
    let folder = tempfile::tempdir().unwrap();
    let path = program(folder.path(), &Fixture { needed: vec!["libc.so.6", "libm.so.6"],
        wants: vec![("libm.so.6", vec!["GLIBC_2.29"]), ("libc.so.6", vec!["GLIBC_2.34", "GLIBC_2.43"])], ..linked(vec![]) });
    let libm = system.path().join("usr/lib").join(this_machine().map(|(_, triplet)| triplet).unwrap_or("x86_64-linux-gnu")).join("libm.so.6");
    std::fs::write(&libm, elf(&Fixture { defines: vec!["libm.so.6", "GLIBC_2.2.5"], ..Default::default() })).unwrap();
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Version { library: "libc.so.6".into(),
        needed: "GLIBC_2.43".into(), newest: Some("GLIBC_2.42".into()) }), "the newest glibc it asks for is said before an older gap");
}

#[test]
fn a_script_needs_its_interpreter_and_what_env_finds() {
    let system = system();
    std::fs::create_dir_all(system.path().join("usr/bin")).unwrap();
    for name in ["env", "bash"] { std::fs::write(system.path().join("usr/bin").join(name), "").unwrap(); }
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("plugin");
    std::fs::write(&path, "#!/usr/bin/bash\necho\n").unwrap();
    assert_eq!(preflight(&path, system.path()), Ok(()));
    std::fs::write(&path, "#!/usr/bin/env python3\nprint()\n").unwrap();
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Interpreter("/usr/bin/python3".into())));
    std::fs::write(system.path().join("usr/bin/python3"), "").unwrap();
    assert_eq!(preflight(&path, system.path()), Ok(()));
    std::fs::write(&path, "#!/usr/bin/perl\n").unwrap();
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Interpreter("/usr/bin/perl".into())));
}

#[test]
fn what_this_system_cannot_run_is_not_a_program_here() {
    let system = system();
    let folder = tempfile::tempdir().unwrap();
    for fixture in [Fixture { class: Some(1), ..linked(vec![]) }, Fixture { machine: Some(0x1234), ..linked(vec![]) }] {
        let path = program(folder.path(), &fixture);
        assert_eq!(preflight(&path, system.path()), Err(Incompatible::Format));
    }
    let path = folder.path().join("plugin");
    for bytes in [&b""[..], b"\x7fELF", b"not a program"] {
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(preflight(&path, system.path()), Err(Incompatible::Format));
    }
    let mut truncated = elf(&linked(vec!["GLIBC_2.34"]));
    truncated.truncate(200);
    std::fs::write(&path, truncated).unwrap();
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Format), "tables that point past the end are refused");
}

#[test]
fn names_copied_out_of_one_program_have_a_budget_of_their_own() {
    let folder = tempfile::tempdir().unwrap();
    let long: &'static str = Box::leak(format!("lib{}.so", "x".repeat(16 << 10)).into_boxed_str());
    // Each table is small; the same 16 KiB name, named 600 times, is not.
    let path = program(folder.path(), &Fixture { needed: vec![long], again: 599, ..Default::default() });
    assert!(read(&path).is_err(), "600 copies of one 16 KiB name are refused before they are made");
    let path = program(folder.path(), &Fixture { needed: vec!["libc.so.6"], again: 5000, ..Default::default() });
    assert!(read(&path).is_err(), "so are more names than any program has");
    let path = program(folder.path(), &Fixture { needed: vec!["libc.so.6"], again: 40, ..Default::default() });
    assert!(read(&path).is_ok(), "a program of ordinary size is read");
}

#[test]
fn an_address_whose_translation_overflows_is_malformed_not_a_panic() {
    let folder = tempfile::tempdir().unwrap();
    let mut bytes = elf(&linked(vec!["GLIBC_2.34"]));
    // The first program header is the PT_LOAD: its offset field.
    bytes[64 + 8..64 + 16].copy_from_slice(&u64::MAX.to_le_bytes());
    let path = folder.path().join("plugin");
    std::fs::write(&path, bytes).unwrap();
    assert!(read(&path).is_err());
}

#[test]
fn a_library_is_looked_for_only_where_the_plugin_s_sandbox_sees() {
    let system = system();
    let base = tempfile::tempdir().unwrap();
    let package = base.path().join("package");
    std::fs::create_dir_all(base.path().join("outside")).unwrap();
    let outside = base.path().join("outside/libbundled.so.1");
    std::fs::write(&outside, elf(&Fixture::default())).unwrap();
    let path = program(&package, &Fixture { needed: vec!["libbundled.so.1", "libc.so.6"], runpath: Some("$ORIGIN/../outside".into()),
        ..linked(vec!["GLIBC_2.34"]) });
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Library("libbundled.so.1".into())),
        "$ORIGIN/.. leaves the package, and the sandbox does not see where it leads");
    let named: &'static str = Box::leak(outside.display().to_string().into_boxed_str());
    let path = program(&package, &Fixture { needed: vec![named], ..Default::default() });
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Library(named.into())),
        "a path named whole is looked for under the sandbox's root, not Chilpa's");
    let triplet = this_machine().map(|(_, triplet)| triplet).unwrap_or("x86_64-linux-gnu");
    let inside: &'static str = Box::leak(format!("/usr/lib/{triplet}/libc.so.6").into_boxed_str());
    let path = program(&package, &Fixture { needed: vec![inside], ..Default::default() });
    assert_eq!(preflight(&path, system.path()), Ok(()), "one under the runtime's /usr is found there");
}

#[test]
fn the_libraries_one_preflight_reads_share_one_budget() {
    // Each library within its own budget, twenty of them together past the
    // preflight's: a package can bundle as many libraries as it likes.
    let system = system();
    let triplet = this_machine().map(|(_, triplet)| triplet).unwrap_or("x86_64-linux-gnu");
    let libraries = system.path().join("usr/lib").join(triplet);
    let filler: &'static str = Box::leak("v".repeat(900 << 10).into_boxed_str());
    let mut needed = Vec::new();
    let mut wants = Vec::new();
    for index in 0..20 {
        let name: &'static str = Box::leak(format!("libpart{index}.so.1").into_boxed_str());
        let version: &'static str = Box::leak(format!("PART_{index}").into_boxed_str());
        std::fs::write(libraries.join(name), elf(&Fixture { defines: vec![filler, version], ..Default::default() })).unwrap();
        needed.push(name);
        wants.push((name, vec![version]));
    }
    let folder = tempfile::tempdir().unwrap();
    let path = program(folder.path(), &Fixture { needed: needed.clone(), wants: wants.clone(), ..Default::default() });
    assert_eq!(preflight(&path, system.path()), Err(Incompatible::Format), "18 MiB of names in all is over the 16 MiB of one preflight");
    let path = program(folder.path(), &Fixture { needed: needed[..10].to_vec(), wants: wants[..10].to_vec(), ..Default::default() });
    assert_eq!(preflight(&path, system.path()), Ok(()), "nine MiB in all is within it");
}

/// The named plugins against a real runtime, when one is given:
/// `CHILPA_PREFLIGHT_RUNTIME` is the runtime's `files` folder,
/// `CHILPA_PREFLIGHT_PLUGINS` a folder of unpacked packages, and
/// `CHILPA_PREFLIGHT_REPORT` where each one's result is written.
#[test]
fn real_plugins_against_a_real_runtime_when_given() {
    let (Some(runtime), Some(plugins), Some(report)) = (std::env::var_os("CHILPA_PREFLIGHT_RUNTIME"),
        std::env::var_os("CHILPA_PREFLIGHT_PLUGINS"), std::env::var_os("CHILPA_PREFLIGHT_REPORT")) else { return };
    // A runtime's files are its /usr; the root links into it as Flatpak's does.
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&runtime, root.path().join("usr")).unwrap();
    for (link, target) in [("lib", "usr/lib"), ("lib64", "usr/lib64"), ("bin", "usr/bin")] {
        std::os::unix::fs::symlink(target, root.path().join(link)).unwrap();
    }
    let mut lines = Vec::new();
    let mut packages: Vec<_> = std::fs::read_dir(plugins).unwrap().map(|entry| entry.unwrap().path()).collect();
    packages.sort();
    for package in packages {
        let program = package.join("plugin");
        let newest = match read(&program) { Ok(Kind::Program(found)) => found.newest_glibc().unwrap_or("none").to_string(), _ => "-".into() };
        let result = if program.exists() { format!("{:?}", preflight(&program, root.path())) } else { "no process program".into() };
        lines.push(format!("{}: {result} (newest glibc asked: {newest})", package.file_name().unwrap().to_string_lossy()));
    }
    std::fs::write(report, lines.join("\n") + "\n").unwrap();
}
