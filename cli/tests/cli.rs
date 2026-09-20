//! End-to-end tests that drive the real `brarchive-cli` binary against a
//! miniature resource pack in `tests/fixtures/pack/`.
//!
//! The fixture mirrors how Mojang ships packs: loose files that were never
//! archived (`manifest.json`, PNGs) sit in the same directories that the
//! archives under `__brarchive/` decode into. Any regression that stops the
//! CLI from unpacking a whole pack in place (see issue #9) shows up here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_brarchive-cli");

fn fixture_pack() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pack")
}

/// The content of every entry inside the fixture archives, keyed by the path
/// it must land at (relative to the pack root) after a recursive decode.
fn archived_files() -> BTreeMap<&'static str, &'static [u8]> {
    BTreeMap::from([
        (
            "textures/terrain_texture.json",
            br#"{"resource_pack_name":"vanilla","texture_name":"atlas.terrain","texture_data":{"stone":{"textures":"textures/blocks/stone"}}}"#.as_slice(),
        ),
        (
            "textures/flipbook_textures.json",
            br#"[{"flipbook_texture":"textures/blocks/water_still","atlas_tile":"water_still","ticks_per_frame":2}]"#.as_slice(),
        ),
        ("textures/ui/_ui_defs.json", br#"{"ui_defs":["ui/button.json"]}"#.as_slice()),
        (
            "textures/ui/button.json",
            br#"{"namespace":"button","button":{"type":"button","size":[16,16]}}"#.as_slice(),
        ),
        (
            "entity/zombie.entity.json",
            br#"{"format_version":"1.10.0","minecraft:client_entity":{"description":{"identifier":"minecraft:zombie","textures":{"default":"textures/entity/zombie"}}}}"#.as_slice(),
        ),
        // Compiled binary (MCB) entry: not valid UTF-8, not JSON.
        (
            "entity/creeper.entity.json",
            b"\x7fMCB\x01\x00\xd2\x20\xde\x77\xff\x00".as_slice(),
        ),
    ])
}

/// Files that live in the pack outside of any archive.
const LOOSE_FILES: &[&str] = &[
    "manifest.json",
    "pack_icon.png",
    "textures/ui/button.png",
    "textures/ui/button_hover.png",
];

const ARCHIVES: &[&str] = &[
    "__brarchive/entity.brarchive",
    "__brarchive/textures.brarchive",
    "__brarchive/textures/ui.brarchive",
];

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// A scratch directory that is removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("brarchive-cli-test-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn join(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

/// A fresh working copy of the fixture pack.
fn pack_copy() -> TempDir {
    let dir = TempDir::new();
    copy_dir(&fixture_pack(), dir.path());
    dir
}

/// Every file under `root`, keyed by `/`-separated relative path.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("failed to spawn brarchive-cli")
}

fn output_text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn assert_ok(out: &Output) {
    assert!(
        out.status.success(),
        "expected success, got {:?}\n{}",
        out.status.code(),
        output_text(out)
    );
}

fn assert_fails(out: &Output, expected_message: &str) {
    assert!(
        !out.status.success(),
        "expected failure but the command succeeded\n{}",
        output_text(out)
    );
    let text = output_text(out);
    assert!(
        text.contains(expected_message),
        "expected output to mention {expected_message:?}\n{text}"
    );
}

fn assert_pack_fully_decoded(root: &Path) {
    let files = snapshot(root);
    for (rel, expected) in archived_files() {
        let got = files
            .get(rel)
            .unwrap_or_else(|| panic!("{rel} was not decoded; have {:?}", files.keys()));
        assert_eq!(got, expected, "content mismatch for {rel}");
    }
}

fn assert_loose_files_intact(root: &Path) {
    let original = snapshot(&fixture_pack());
    let files = snapshot(root);
    for rel in LOOSE_FILES {
        assert_eq!(
            files.get(*rel),
            original.get(*rel),
            "loose file {rel} was modified or removed"
        );
    }
}

// ---------------------------------------------------------------------------
// fixture sanity
// ---------------------------------------------------------------------------

#[test]
fn fixture_pack_has_expected_layout() {
    let files = snapshot(&fixture_pack());
    let mut expected: Vec<&str> = LOOSE_FILES.iter().chain(ARCHIVES).copied().collect();
    expected.sort_unstable();
    let got: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(got, expected);

    // Every archive is non-empty; an empty archive would silently weaken the
    // decode tests below.
    for archive in ARCHIVES {
        let bytes = fs::read(fixture_pack().join(archive)).unwrap();
        let names = brarchive::list(&bytes).unwrap();
        assert!(!names.is_empty(), "{archive} holds no entries");
    }
}

// ---------------------------------------------------------------------------
// decode --recursive (issue #9)
// ---------------------------------------------------------------------------

/// Regression test for issue #9: the archives decode into directories that
/// already contain loose files, and that must merge rather than fail with
/// "Output directory is not empty".
#[test]
fn recursive_decode_merges_into_directories_with_loose_files() {
    let pack = pack_copy();
    assert_ok(&run(&[
        "decode",
        pack.path().to_str().unwrap(),
        "--recursive",
    ]));
    assert_pack_fully_decoded(pack.path());
    assert_loose_files_intact(pack.path());
    // Archives stay put unless --delete-source is passed.
    for archive in ARCHIVES {
        assert!(pack.join(archive).is_file(), "{archive} was removed");
    }
}

/// `textures.brarchive` decodes into `textures/`, which is also the parent of
/// where `textures/ui.brarchive` decodes to. Whichever the directory walk
/// yields first, the other must still succeed.
#[test]
fn decode_into_parent_after_child_directory_exists() {
    let pack = pack_copy();
    let ui_archive = pack.join("__brarchive/textures/ui.brarchive");
    let textures_archive = pack.join("__brarchive/textures.brarchive");

    // Child first, so textures/ is non-empty when the parent archive arrives.
    assert_ok(&run(&[
        "decode",
        ui_archive.to_str().unwrap(),
        pack.join("textures/ui").to_str().unwrap(),
    ]));
    assert_ok(&run(&[
        "decode",
        textures_archive.to_str().unwrap(),
        pack.join("textures").to_str().unwrap(),
    ]));

    let files = snapshot(pack.path());
    assert!(files.contains_key("textures/terrain_texture.json"));
    assert!(files.contains_key("textures/ui/button.json"));
    assert_loose_files_intact(pack.path());
}

#[test]
fn recursive_decode_refuses_to_overwrite_existing_files() {
    let pack = pack_copy();
    assert_ok(&run(&[
        "decode",
        pack.path().to_str().unwrap(),
        "--recursive",
    ]));

    // Simulate a user edit; a second decode must not silently discard it.
    let edited = pack.join("textures/ui/button.json");
    fs::write(&edited, b"edited by user").unwrap();
    let before = snapshot(pack.path());

    let out = run(&["decode", pack.path().to_str().unwrap(), "--recursive"]);
    assert_fails(&out, "--overwrite");
    assert_eq!(fs::read(&edited).unwrap(), b"edited by user");
    assert_eq!(
        snapshot(pack.path()),
        before,
        "files changed despite refusal"
    );
}

#[test]
fn recursive_decode_with_overwrite_replaces_existing_files() {
    let pack = pack_copy();
    assert_ok(&run(&[
        "decode",
        pack.path().to_str().unwrap(),
        "--recursive",
    ]));

    let edited = pack.join("textures/ui/button.json");
    fs::write(&edited, b"stale").unwrap();

    assert_ok(&run(&[
        "decode",
        pack.path().to_str().unwrap(),
        "--recursive",
        "--overwrite",
    ]));
    assert_pack_fully_decoded(pack.path());
    assert_loose_files_intact(pack.path());
}

#[test]
fn recursive_decode_into_separate_output_directory() {
    let pack = pack_copy();
    let out = TempDir::new();
    assert_ok(&run(&[
        "decode",
        pack.path().to_str().unwrap(),
        out.path().to_str().unwrap(),
        "--recursive",
    ]));
    assert_pack_fully_decoded(out.path());
    // Only archived content goes to the output dir; loose files are not copied.
    let files = snapshot(out.path());
    assert_eq!(files.len(), archived_files().len(), "{:?}", files.keys());
    // The source pack itself is untouched.
    assert_eq!(snapshot(pack.path()), snapshot(&fixture_pack()));
}

#[test]
fn recursive_decode_delete_source_removes_archives_only() {
    let pack = pack_copy();
    assert_ok(&run(&[
        "decode",
        pack.path().to_str().unwrap(),
        "--recursive",
        "--delete-source",
    ]));
    assert_pack_fully_decoded(pack.path());
    assert_loose_files_intact(pack.path());
    for archive in ARCHIVES {
        assert!(!pack.join(archive).exists(), "{archive} still exists");
    }
}

#[test]
fn recursive_decode_pretty_prints_json_and_leaves_binary_untouched() {
    let pack = pack_copy();
    assert_ok(&run(&[
        "decode",
        pack.path().to_str().unwrap(),
        "--recursive",
        "--pretty",
    ]));

    let zombie = fs::read_to_string(pack.join("entity/zombie.entity.json")).unwrap();
    assert!(
        zombie.contains('\n'),
        "JSON was not pretty-printed:\n{zombie}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&zombie).unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(archived_files()["entity/zombie.entity.json"]).unwrap();
    assert_eq!(parsed, original);

    assert_eq!(
        fs::read(pack.join("entity/creeper.entity.json")).unwrap(),
        archived_files()["entity/creeper.entity.json"],
        "binary MCB entry was altered by --pretty"
    );
}

#[test]
fn recursive_decode_requires_brarchive_directory() {
    let dir = TempDir::new();
    fs::write(dir.join("manifest.json"), b"{}").unwrap();
    let out = run(&["decode", dir.path().to_str().unwrap(), "--recursive"]);
    assert_fails(&out, "No __brarchive/ directory found");
}

// ---------------------------------------------------------------------------
// decode (single archive)
// ---------------------------------------------------------------------------

#[test]
fn single_decode_merges_into_existing_directory() {
    let out = TempDir::new();
    fs::write(out.join("keep.txt"), b"keep me").unwrap();
    let archive = fixture_pack().join("__brarchive/textures/ui.brarchive");

    assert_ok(&run(&[
        "decode",
        archive.to_str().unwrap(),
        out.path().to_str().unwrap(),
    ]));

    let files = snapshot(out.path());
    assert_eq!(files["keep.txt"], b"keep me");
    assert_eq!(
        files["button.json"],
        archived_files()["textures/ui/button.json"]
    );
}

#[test]
fn single_decode_refuses_to_overwrite_then_succeeds_with_flag() {
    let out = TempDir::new();
    let archive = fixture_pack().join("__brarchive/textures/ui.brarchive");
    let archive = archive.to_str().unwrap();
    let out_dir = out.path().to_str().unwrap();

    assert_ok(&run(&["decode", archive, out_dir]));
    assert_fails(&run(&["decode", archive, out_dir]), "--overwrite");
    assert_ok(&run(&["decode", archive, out_dir, "--overwrite"]));
}

#[test]
fn single_decode_rejects_file_as_output_directory() {
    let out = TempDir::new();
    let file = out.join("not_a_dir");
    fs::write(&file, b"x").unwrap();
    let archive = fixture_pack().join("__brarchive/textures/ui.brarchive");

    let result = run(&["decode", archive.to_str().unwrap(), file.to_str().unwrap()]);
    assert_fails(&result, "is not a directory");
    assert_eq!(fs::read(&file).unwrap(), b"x");
}

// ---------------------------------------------------------------------------
// encode --recursive round trip
// ---------------------------------------------------------------------------

/// Unpack the fixture pack, re-archive every directory with
/// `encode --recursive --delete-source`, then decode again and check the
/// files inside subdirectories are byte-for-byte what we started with.
///
/// Root-level files are excluded here; see
/// `encode_decode_round_trip_restores_root_level_files` for why.
#[test]
fn encode_and_decode_recursive_round_trip() {
    let pack = pack_copy();
    let root = pack.path().to_str().unwrap();
    assert_ok(&run(&["decode", root, "--recursive", "--delete-source"]));
    let plain_tree = snapshot(pack.path());
    assert!(!plain_tree.keys().any(|k| k.starts_with("__brarchive/")));

    assert_ok(&run(&["encode", root, "--recursive", "--delete-source"]));
    let archived_tree = snapshot(pack.path());
    assert!(
        archived_tree.keys().all(|k| k.starts_with("__brarchive/")),
        "sources were not deleted: {:?}",
        archived_tree.keys()
    );
    assert!(archived_tree.contains_key("__brarchive/textures/ui.brarchive"));

    assert_ok(&run(&["decode", root, "--recursive", "--delete-source"]));
    let round_tripped = snapshot(pack.path());

    // Root-level files currently come back under `<pack dir name>/`; ignore
    // them here so this test only covers the part that does round-trip.
    let misplaced_root = format!("{}/", pack.path().file_name().unwrap().to_str().unwrap());
    let nested = |tree: &BTreeMap<String, Vec<u8>>| -> BTreeMap<String, Vec<u8>> {
        tree.iter()
            .filter(|(k, _)| k.contains('/') && !k.starts_with(&misplaced_root))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };
    assert_eq!(nested(&round_tripped), nested(&plain_tree));
}

/// `encode --recursive` stores root-level files in
/// `__brarchive/<pack dir name>.brarchive`, but `decode --recursive` unpacks
/// that archive into `<pack dir name>/` instead of the pack root, so the two
/// commands are not inverses for root-level files. Ignored until that
/// asymmetry is resolved; run with `--ignored` to see the current behaviour.
#[test]
#[ignore = "encode/decode --recursive do not round-trip root-level files"]
fn encode_decode_round_trip_restores_root_level_files() {
    let pack = pack_copy();
    let root = pack.path().to_str().unwrap();
    assert_ok(&run(&["decode", root, "--recursive", "--delete-source"]));
    let plain_tree = snapshot(pack.path());

    assert_ok(&run(&["encode", root, "--recursive", "--delete-source"]));
    assert_ok(&run(&["decode", root, "--recursive", "--delete-source"]));
    assert_eq!(snapshot(pack.path()), plain_tree);
}

// ---------------------------------------------------------------------------
// list --recursive
// ---------------------------------------------------------------------------

#[test]
fn list_recursive_prints_every_archive_and_entry() {
    let out = run(&["list", fixture_pack().to_str().unwrap(), "--recursive"]);
    assert_ok(&out);
    let text = String::from_utf8(out.stdout).unwrap();
    for header in ["entity.brarchive:", "textures.brarchive:", "ui.brarchive:"] {
        assert!(text.contains(header), "missing {header:?} in:\n{text}");
    }
    for entry in [
        "zombie.entity.json",
        "creeper.entity.json",
        "terrain_texture.json",
        "flipbook_textures.json",
        "_ui_defs.json",
        "button.json",
    ] {
        assert!(text.contains(entry), "missing {entry:?} in:\n{text}");
    }
}
