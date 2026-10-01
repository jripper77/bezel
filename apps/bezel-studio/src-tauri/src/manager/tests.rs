//! The storage manager's commands on the fake screens (in-memory storage),
//! with the catalog in a `MemoryArchive` (a `DiskArchive` for thumbnails).

use std::path::{Path, PathBuf};

use bezel_core::BezelError;
use bezel_core::app::manager::Halt;
use bezel_core::domain::archive::{Catalog, EntryState, ScreenKey, ScreenRecord};
use bezel_core::domain::device::{ModelId, Transport, UsbId};
use bezel_core::domain::discovery::{DeviceAddress, Endpoint};
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::job::JobPhase;
use bezel_core::domain::screen::Confirm;
use bezel_core::domain::storage::FileEntry;
use bezel_core::domain::theme::{AssetRef, Background, Theme};
use bezel_devices::FakeBus;
use bezel_devices::fake::{FAKE_UPLOAD_CHUNK, FakeStorage, StorageCall};
use bezel_media::archive::{DiskArchive, MemoryArchive, content_id};
use serde_json::json;

use super::*;
use crate::dto::ProgressDto;
use crate::messages::UiResult;
use crate::storage::tests::{FakeMedia, Fixture, KEY, TIME, fixture_on, remote_path};

/// The user's card (29.7 GiB).
const CARD: u64 = 31_890_132_172;

fn fixture(name: &str, storage: FakeStorage) -> Fixture {
    let copies = Copies::in_memory(MemoryArchive::new());
    fixture_on(
        &format!("manager-{name}"),
        FakeBus::turing_88(),
        storage,
        FakeMedia::ready(),
        copies,
    )
}

fn card(name: &str) -> Fixture {
    fixture(name, FakeStorage::default().with_card(CARD))
}

fn catalog(f: &Fixture) -> Catalog {
    f.backend.storage.archive().load().unwrap()
}

fn record(f: &Fixture) -> ScreenRecord {
    let key = ScreenKey::new(ModelId("turing-8.8"));
    catalog(f).screen(&key).cloned().unwrap_or_default()
}

/// Sends a local file of `bytes` bytes named `name` to `medium` through the
/// storage tab; its screen path.
fn send(f: &Fixture, name: &str, bytes: usize, medium: &str) -> String {
    let ready = f.ready(&f.local(name, bytes), medium);
    f.run(ready.ticket, Confirm::No).0.unwrap();
    ready.target.path
}

fn ready_plan(plan: UiResult<PlanDto>) -> PlanReadyDto {
    match plan.unwrap() {
        PlanDto::Ready(ready) => ready,
        PlanDto::Refused(refused) => panic!("refused: {refused:?}"),
    }
}

fn refused_plan(plan: UiResult<PlanDto>) -> PlanRefusedDto {
    match plan.unwrap() {
        PlanDto::Refused(refused) => refused,
        PlanDto::Ready(ready) => panic!("not refused: {ready:?}"),
    }
}

fn ask_move(paths: &[&str], to: &str) -> Ask {
    Ask::Move {
        paths: paths.iter().map(ToString::to_string).collect(),
        to: to.into(),
    }
}

fn plan_of(f: &Fixture, ask: &Ask) -> UiResult<PlanDto> {
    f.backend.plan_transfer(KEY, ask, &[], TIME)
}

/// The report of a run that ran.
fn ran(run: RunDto) -> TransferReportDto {
    match run {
        RunDto::Ran(report) => *report,
        RunDto::Refused(refused) => panic!("refused: {refused:?}"),
    }
}

fn run(f: &Fixture, ticket: u64) -> (UiResult<TransferReportDto>, Vec<ProgressDto>) {
    let mut seen = Vec::new();
    let report = f
        .backend
        .run_plan(ticket, Confirm::Yes, TIME, &mut |p| seen.push(p));
    (report.map(ran), seen)
}

/// Deletes `path` behind Bezel's back (another app, a format on the PC).
fn delete_elsewhere(f: &Fixture, path: &str) {
    let mut link = bezel_core::app::open_screen(&FakeBus::turing_88(), &f.connector, None).unwrap();
    bezel_core::app::storage::delete(link.as_mut(), &remote_path(path), Confirm::Yes).unwrap();
}

/// Stores `bytes` bytes at `path` behind Bezel's back (another app).
fn store_elsewhere(f: &Fixture, path: &str, bytes: usize) {
    let mut link = bezel_core::app::open_screen(&FakeBus::turing_88(), &f.connector, None).unwrap();
    let storage = bezel_core::app::storage::storage_of(link.as_mut()).unwrap();
    let token = bezel_core::domain::job::CancelToken::new();
    let mut sink = |_| {};
    let mut job = bezel_core::domain::job::Job::new(&token, &mut sink);
    storage
        .upload(&remote_path(path), &vec![7; bytes], &mut job)
        .unwrap();
}

/// The bytes `Fixture::local` writes for a file of `bytes` bytes.
fn local_bytes(bytes: usize) -> Vec<u8> {
    (0..bytes).map(|i| (i % 251) as u8).collect()
}

/// (file index, phase, done, total) of the reports other than upload bytes.
fn milestones(seen: &[ProgressDto]) -> Vec<(usize, &'static str, u64, u64)> {
    seen.iter()
        .filter(|p| p.phase != "upload")
        .map(|p| (p.step.as_ref().unwrap().index, p.phase, p.done, p.total))
        .collect()
}

#[test]
fn the_overview_shows_entries_findings_and_protected_files() {
    let file = |path: &str, byte: u8, size: usize| (remote_path(path), vec![byte; size]);
    let mut storage = FakeStorage::default().with_card(CARD);
    for (path, data) in [
        file("sd/video/demon_open.mp4", 1, 1000),
        file("sd/video/demon_open.mp4.mp4", 1, 1000),
        file("sd/video/NVI.mp4", 2, 2000),
        file("sd/video/NVI.mp427034822.mp4", 2, 2500),
        file("sd/video/m04.mp4", 3, 700),
        file("internal/video/intro.mp4", 4, 900),
    ] {
        storage = storage.with_file(path, data);
    }
    let f = fixture("overview", storage);
    let mut theme = Theme::blank("Video", Size::new(480, 1920), Orientation::Landscape);
    theme.background = Background::Video {
        asset: AssetRef("assets/intro.mp4".into()),
        poster: None,
    };
    f.backend.studio().set_theme(theme);
    let before = crate::clock::unix_seconds();
    let sent = send(&f, "native.mp4", 3000, "internal");
    f.backend
        .set_boot_media(KEY, Some("sd/video/m04.mp4"), None, Confirm::Yes, TIME)
        .unwrap();
    let writes = f.writes().len();

    let dto = f.backend.manager_overview(KEY, TIME).unwrap();
    assert_eq!(f.writes().len(), writes, "the overview only asks");
    let at = |path: &str| dto.files.iter().find(|f| f.file.path == path).unwrap();
    let ours = at(&sent);
    let entry = ours.entry.as_ref().unwrap();
    assert_eq!((entry.state, entry.local_copy), ("stored", true));
    assert!(entry.sent_at >= before);
    assert!(entry.source.as_deref().unwrap().ends_with("native.mp4"));
    assert_eq!((ours.finding.as_ref(), ours.protected), (None, None));

    let duplicate = at("sd/video/demon_open.mp4.mp4").finding.clone().unwrap();
    assert_eq!(
        (
            duplicate.code,
            duplicate.prechecked,
            duplicate.kept.as_deref()
        ),
        ("duplicate", true, Some("sd/video/demon_open.mp4"))
    );
    let variant = at("sd/video/NVI.mp427034822.mp4").finding.clone().unwrap();
    assert_eq!(
        (variant.code, variant.prechecked, variant.kept.as_deref()),
        ("variant", false, Some("sd/video/NVI.mp4"))
    );
    assert_eq!(
        at("sd/video/demon_open.mp4").finding.as_ref().unwrap().code,
        "unused"
    );
    let boot = at("sd/video/m04.mp4");
    assert_eq!(
        (boot.protected, boot.finding.as_ref()),
        (Some("boot"), None)
    );
    let video = at("internal/video/intro.mp4");
    assert_eq!(
        (video.protected, video.finding.as_ref()),
        (Some("themeVideo"), None)
    );
    assert_eq!(at("sd/video/NVI.mp4").entry, None, "Bezel did not send it");

    assert_eq!(dto.internal.used, 3000 + 900);
    assert_eq!(dto.card.unwrap().total, CARD);
    assert!(dto.deletes);
    assert_eq!(dto.cap, bezel_core::domain::storage::REV_C_MAX_UPLOAD_BYTES);
    assert_eq!((dto.cache.copies, dto.cache.bytes), (1, 3000));
    assert!(dto.restorable.is_empty());
    let json = serde_json::to_value(&dto).unwrap();
    let first = &json["files"][0];
    assert!(first["path"].is_string() && first["medium"].is_string());
    assert_eq!(json["folderErrors"], json!([]));
    assert_eq!(json["cache"]["deletedCopies"], 0);
    let ours = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == sent.as_str())
        .unwrap();
    assert_eq!(ours["entry"]["localCopy"], true);
    assert_eq!(ours["size"], 3000);

    // A file deleted behind Bezel's back is restorable.
    delete_elsewhere(&f, &sent);
    let dto = f.backend.manager_overview(KEY, TIME).unwrap();
    let back = &dto.restorable[0];
    assert_eq!(
        (
            back.id.as_str(),
            back.state,
            back.local_copy,
            back.other_card
        ),
        ("internal/video/native.mp4@", "missing", true, false)
    );
    let json = serde_json::to_value(back).unwrap();
    assert_eq!(
        (json["medium"].as_str(), json["size"].as_u64()),
        (Some("internal"), Some(3000))
    );
    assert!(json["sentAt"].is_u64() && json["otherCard"] == false);
    assert_eq!(
        record(&f).entries[0].state,
        EntryState::Missing,
        "saved reconciled"
    );
}

#[test]
fn a_move_reports_by_code() {
    let f = card("move");
    let a = send(&f, "native-a.mp4", FAKE_UPLOAD_CHUNK + 10, "internal");
    let b = send(&f, "native-b.mp4", 500, "internal");
    let ready = ready_plan(plan_of(&f, &ask_move(&[&a, &b], "sd")));
    assert_eq!((ready.transfer, ready.to), ("move", "sd"));
    assert_eq!(ready.steps.len(), 2);
    assert_eq!(ready.steps[0].target, "sd/video/native-a.mp4");
    assert_eq!(ready.bytes, FAKE_UPLOAD_CHUNK as u64 + 510);
    assert_eq!(ready.free, CARD);
    let json = serde_json::to_value(PlanDto::Ready(ready.clone())).unwrap();
    assert_eq!(json["status"], "ready");
    assert_eq!(json["steps"][0]["replaces"], serde_json::Value::Null);

    let before = f.writes().len();
    let mut busy = None;
    let mut seen = Vec::new();
    let report = f
        .backend
        .run_plan(ready.ticket, Confirm::Yes, TIME, &mut |p| {
            if busy.is_none() {
                busy = Some(f.backend.cache_info().unwrap_err().code());
            }
            seen.push(p);
        });
    let report = ran(report.unwrap());
    assert_eq!(busy, Some("busy"), "one storage operation at a time");
    assert_eq!(report.done.len(), 2);
    assert_eq!(
        (report.failed.as_ref(), report.cancelled.as_ref()),
        (None, None)
    );
    // Each file: its copy sent and checked, then its source deleted.
    let writes: Vec<StorageCall> = f.writes()[before..].to_vec();
    assert!(
        matches!(&writes[0], StorageCall::Upload(p, _) if p.to_string() == "sd/video/native-a.mp4")
    );
    assert_eq!(writes[1], StorageCall::Delete(remote_path(&a)));
    assert_eq!(writes[3], StorageCall::Delete(remote_path(&b)));
    assert_eq!(
        milestones(&seen),
        vec![
            (0, "verify", 0, 1),
            (0, "verify", 1, 1),
            (0, "delete", 0, 1),
            (0, "delete", 1, 1),
            (1, "verify", 0, 1),
            (1, "verify", 1, 1),
            (1, "delete", 0, 1),
            (1, "delete", 1, 1),
        ]
    );
    let first = serde_json::to_value(&seen[0]).unwrap();
    assert_eq!(
        first,
        json!({"phase": "upload", "done": 0, "total": FAKE_UPLOAD_CHUNK + 10,
               "step": {"index": 0, "count": 2, "source": a, "target": "sd/video/native-a.mp4"}})
    );
    let entries = record(&f).entries;
    let paths: Vec<String> = entries.iter().map(|e| e.path.to_string()).collect();
    assert_eq!(paths, ["sd/video/native-a.mp4", "sd/video/native-b.mp4"]);
    assert!(
        entries
            .iter()
            .all(|e| e.state == EntryState::Stored && e.card == Some(CARD))
    );

    // Cancelled during the first upload: its source stays, the partial file
    // is reported, the rest never starts.
    f.backend.manager_overview(KEY, TIME).unwrap();
    let back = ready_back(&f);
    let (report, _) = {
        let mut seen = Vec::new();
        let report = f
            .backend
            .run_plan(back.ticket, Confirm::Yes, TIME, &mut |p| {
                if p.phase == "upload" && p.done > 0 {
                    assert!(f.backend.cancel_job());
                }
                seen.push(p);
            });
        (ran(report.unwrap()), seen)
    };
    let cancelled = report.cancelled.clone().unwrap();
    assert_eq!(cancelled.step.source, "sd/video/native-a.mp4");
    assert_eq!(cancelled.partial, Some(FAKE_UPLOAD_CHUNK as u64));
    assert_eq!(cancelled.stage, "upload");
    assert_eq!(report.not_started.len(), 1);
    assert!(
        f.storage()
            .files
            .contains_key(&remote_path("sd/video/native-a.mp4"))
    );
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["notStarted"][0]["source"], "sd/video/native-b.mp4");
    assert!(!f.backend.cancel_job(), "the job ended");

    // A target without room: the preflight's refusal, by code.
    let tight = fixture("move-full", FakeStorage::default().with_card(5000));
    let big = send(&tight, "native-big.mp4", 6000, "internal");
    let ready = ready_plan(plan_of(&tight, &ask_move(&[&big], "sd")));
    let before = tight.writes().len();
    let report = run(&tight, ready.ticket).0.unwrap();
    let failed = report.failed.clone().unwrap();
    assert_eq!((failed.stage, failed.why.halt), ("preflight", "refused"));
    let refusal = failed.why.refusal.unwrap();
    assert_eq!(
        (refusal.code, refusal.bytes, refusal.limit),
        ("noSpace", Some(6000), Some(5000))
    );
    assert_eq!(failed.why.error, None);
    assert_eq!(
        tight.writes().len(),
        before,
        "nothing sent, nothing deleted"
    );

    // A source deleted since the plan: the run stops, the code says why.
    let small = fixture("move-gone", FakeStorage::default().with_card(CARD));
    let gone = send(&small, "native-gone.mp4", 400, "internal");
    let plan_gone = ready_plan(plan_of(&small, &ask_move(&[&gone], "sd")));
    delete_elsewhere(&small, &gone);
    let report = run(&small, plan_gone.ticket).0.unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(
        json["failed"],
        json!({"step": {"source": gone, "target": "sd/video/native-gone.mp4", "size": 400,
                        "replaces": null},
               "stage": "preflight", "halt": "sourceChanged", "error": null,
               "refusal": null, "conflict": null}),
        "a code, no English to show"
    );

    // A failure carries the backend's error by code.
    let failed = FailedStepDto {
        step: report.failed.unwrap().step,
        stage: "upload",
        why: HaltDto::from(&Halt::Failed(BezelError::Hung("no answer".into()))),
    };
    let json = serde_json::to_value(&failed).unwrap();
    assert_eq!(
        (json["halt"].as_str(), json["error"]["code"].as_str()),
        (Some("failed"), Some("hung"))
    );
    // A name taken since the plan names the file there.
    let taken = FileEntry {
        path: remote_path("sd/video/native-gone.mp4"),
        size: Some(9),
    };
    let json = serde_json::to_value(HaltDto::from(&Halt::Conflict(taken))).unwrap();
    assert_eq!(
        (json["halt"].as_str(), json["conflict"]["name"].as_str()),
        (Some("conflict"), Some("native-gone.mp4"))
    );
    let json = serde_json::to_value(HaltDto::from(&Halt::NoLocalCopy)).unwrap();
    assert_eq!(
        json,
        json!({"halt": "noLocalCopy", "error": null, "refusal": null, "conflict": null})
    );
}

/// Plans moving both files of the card back to the internal flash.
fn ready_back(f: &Fixture) -> PlanReadyDto {
    let paths = ["sd/video/native-a.mp4", "sd/video/native-b.mp4"];
    ready_plan(plan_of(f, &ask_move(&paths, "internal")))
}

#[test]
fn a_plan_runs_only_with_the_dialogs_confirmation() {
    let f = card("unconfirmed");
    let a = send(&f, "native-a.mp4", 700, "internal");
    let ready = ready_plan(plan_of(&f, &ask_move(&[&a], "sd")));
    let calls = f.storage().calls.len();
    let files = f.storage().files.clone();

    let err = f
        .backend
        .run_plan(ready.ticket, Confirm::No, TIME, &mut |_| {
            panic!("no progress")
        })
        .unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    assert_eq!(
        f.storage().calls.len(),
        calls,
        "the screen was not even asked"
    );
    assert_eq!(f.storage().files, files, "nothing sent, nothing deleted");
    assert_eq!(
        record(&f).entries[0].path.to_string(),
        a,
        "the catalog is as it was"
    );
}

#[test]
fn a_plan_runs_once_and_goes_stale_when_the_screen_changes() {
    let f = card("stale");
    let a = send(&f, "native-a.mp4", 700, "internal");
    let first = ready_plan(plan_of(&f, &ask_move(&[&a], "sd")));
    let second = ready_plan(plan_of(&f, &ask_move(&[&a], "sd")));
    assert_eq!(
        run(&f, first.ticket).0.unwrap_err().code(),
        "stale",
        "replaced"
    );
    run(&f, second.ticket).0.unwrap();
    assert_eq!(
        run(&f, second.ticket).0.unwrap_err().code(),
        "stale",
        "runs once"
    );

    // Another job that may change the screen drops the plan.
    let back = ready_plan(plan_of(
        &f,
        &ask_move(&["sd/video/native-a.mp4"], "internal"),
    ));
    send(&f, "native-c.mp4", 300, "internal");
    assert_eq!(run(&f, back.ticket).0.unwrap_err().code(), "stale");
    let back = ready_plan(plan_of(
        &f,
        &ask_move(&["sd/video/native-a.mp4"], "internal"),
    ));
    f.backend
        .delete_stored(KEY, "internal/video/native-c.mp4", Confirm::Yes, TIME)
        .unwrap();
    assert_eq!(run(&f, back.ticket).0.unwrap_err().code(), "stale");
    // A query does not.
    let back = ready_plan(plan_of(
        &f,
        &ask_move(&["sd/video/native-a.mp4"], "internal"),
    ));
    f.backend.manager_overview(KEY, TIME).unwrap();
    assert_eq!(run(&f, back.ticket).0.unwrap().done.len(), 1);
}

#[test]
fn refusals_come_with_their_code_and_arguments() {
    let f = fixture("refusals", FakeStorage::default());
    let a = send(&f, "native-a.mp4", 700, "internal");
    let rename = |new: &str| Ask::Rename {
        path: a.clone(),
        new_name: new.into(),
    };
    let r = refused_plan(plan_of(&f, &rename("a b.mp4")));
    assert_eq!((r.code, &r.args), ("invalidName", &json!({"char": " "})));
    let r = refused_plan(plan_of(&f, &rename("clip.png")));
    assert_eq!(
        (r.code, &r.args),
        ("extensionChanged", &json!({"expected": "mp4"}))
    );
    let r = refused_plan(plan_of(&f, &rename("NATIVE-A.mp4")));
    assert_eq!((r.code, &r.args), ("sameName", &json!({})));
    let r = refused_plan(plan_of(&f, &ask_move(&[&a], "internal")));
    assert_eq!((r.code, &r.args), ("sameMedium", &json!({"path": a})));
    let r = refused_plan(plan_of(&f, &ask_move(&[&a], "sd")));
    assert_eq!(r.code, "noCard");
    let r = refused_plan(plan_of(&f, &ask_move(&["sd/video/x.mp4"], "internal")));
    assert_eq!(
        (r.code, &r.args),
        ("notListed", &json!({"path": "sd/video/x.mp4"}))
    );
    let json = serde_json::to_value(PlanDto::Refused(r)).unwrap();
    assert_eq!(json["status"], "refused");
    assert!(
        json["message"]
            .as_str()
            .unwrap()
            .contains("not on the screen")
    );

    let err = plan_of(&f, &ask_move(&[&a], "cloud")).unwrap_err();
    assert_eq!(
        (err.code(), err.value("medium")),
        ("unknownMedium", Some("cloud"))
    );
    let err = plan_of(&f, &ask_move(&["elsewhere/x.mp4"], "sd")).unwrap_err();
    assert_eq!(err.code(), "invalidInput");
    assert!(
        f.writes()
            .iter()
            .all(|c| !matches!(c, StorageCall::Delete(_)))
    );

    // Restoring more than the medium holds is refused before anything is
    // sent, saying by how much.
    let storage = FakeStorage {
        internal_total: 10_000,
        ..FakeStorage::default()
    };
    let f = fixture("restore-full", storage);
    let one = send(&f, "native-one.mp4", 4000, "internal");
    let two = send(&f, "native-two.mp4", 4001, "internal");
    delete_elsewhere(&f, &one);
    delete_elsewhere(&f, &two);
    send(&f, "native-big.mp4", 5000, "internal");
    let ids: Vec<String> = f
        .backend
        .manager_overview(KEY, TIME)
        .unwrap()
        .restorable
        .iter()
        .map(|r| r.id.clone())
        .collect();
    assert_eq!(ids.len(), 2);
    let before = f.writes().len();
    let restore = Ask::Restore {
        ids: ids.clone(),
        to: "internal".into(),
    };
    let r = refused_plan(plan_of(&f, &restore));
    assert_eq!(
        (r.code, &r.args),
        ("noSpace", &json!({"needed": 8001, "free": 5000}))
    );
    assert_eq!(f.writes().len(), before);
    // One fits: restored, verified, nothing deleted.
    let restore = Ask::Restore {
        ids: ids[..1].to_vec(),
        to: "internal".into(),
    };
    let ready = ready_plan(plan_of(&f, &restore));
    assert_eq!((ready.transfer, ready.steps.len()), ("restore", 1));
    let report = run(&f, ready.ticket).0.unwrap();
    assert_eq!(report.done[0].target, one);
    assert!(
        f.writes()[before..]
            .iter()
            .all(|c| !matches!(c, StorageCall::Delete(_)))
    );

    // Another app filled the medium since the plan: the run is refused by
    // code before anything is sent.
    delete_elsewhere(&f, &one);
    let restore = Ask::Restore {
        ids: ids[..1].to_vec(),
        to: "internal".into(),
    };
    let ready = ready_plan(plan_of(&f, &restore));
    store_elsewhere(&f, "internal/video/other-app.mp4", 2000);
    let before = f.writes().len();
    let run = f
        .backend
        .run_plan(ready.ticket, Confirm::Yes, TIME, &mut |_| {
            panic!("nothing runs")
        })
        .unwrap();
    let RunDto::Refused(refused) = &run else {
        panic!("not refused: {run:?}");
    };
    assert_eq!(
        (refused.code, &refused.args),
        ("noSpace", &json!({"needed": 4000, "free": 3000}))
    );
    assert_eq!(f.writes().len(), before, "nothing sent");
    let json = serde_json::to_value(&run).unwrap();
    assert_eq!(
        (json["status"].as_str(), json["code"].as_str()),
        (Some("refused"), Some("noSpace"))
    );
    let json = serde_json::to_value(RunDto::Ran(Box::new(report))).unwrap();
    assert_eq!(json["status"], "ran");
    assert_eq!(json["transfer"], "restore");
}

#[test]
fn deleting_files_needs_the_confirmation_and_reports_each_file() {
    let f = fixture(
        "delete",
        FakeStorage::default().with_file(remote_path("internal/video/vendor.mp4"), vec![9; 800]),
    );
    let a = send(&f, "native-a.mp4", 700, "internal");
    let b = send(&f, "native-b.mp4", 600, "internal");
    let calls = f.storage().calls.len();
    let paths = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
    let all = paths(&[
        &a,
        "internal/video/vendor.mp4",
        "internal/video/gone.mp4",
        &b,
    ]);

    let err = f
        .backend
        .delete_files(KEY, &all, Confirm::No, TIME, &mut |_| panic!("no progress"))
        .unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    assert_eq!(
        f.storage().calls.len(),
        calls,
        "the screen was not even asked"
    );

    let mut seen = Vec::new();
    let report = f
        .backend
        .delete_files(KEY, &all, Confirm::Yes, TIME, &mut |p| seen.push(p))
        .unwrap();
    assert_eq!(report.deleted, paths(&[&a, "internal/video/vendor.mp4"]));
    assert_eq!(report.freed, 700 + 800);
    let failed = report.failed.clone().unwrap();
    assert_eq!(failed.path, "internal/video/gone.mp4");
    assert_eq!((failed.why.halt, failed.why.error), ("sourceChanged", None));
    assert_eq!(
        (report.cancelled, report.not_started.clone()),
        (false, paths(&[&b]))
    );
    let done: Vec<(&str, u64, u64)> = seen.iter().map(|p| (p.phase, p.done, p.total)).collect();
    assert_eq!(done, [("delete", 0, 4), ("delete", 1, 4), ("delete", 2, 4)]);
    assert_eq!(seen[1].step.as_ref().unwrap().target, None);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["notStarted"], json!([b]));
    assert_eq!(json["failed"]["halt"], "sourceChanged");
    let deleted = record(&f).entries;
    assert_eq!(deleted[0].state, EntryState::Deleted, "{a} marked deleted");
    assert_eq!(deleted[1].state, EntryState::Stored);

    // Cancel: the files left are not started.
    let report = f
        .backend
        .delete_files(KEY, &paths(&[&b]), Confirm::Yes, TIME, &mut |_| {
            assert!(f.backend.cancel_job());
        })
        .unwrap();
    assert!(report.cancelled);
    assert_eq!(
        (report.deleted.len(), report.not_started.clone()),
        (0, paths(&[&b]))
    );
    assert!(f.storage().files.contains_key(&remote_path(&b)));
}

#[test]
fn associating_an_original_gives_the_file_a_copy() {
    let vendor = "sd/video/clip.mp4";
    let storage = FakeStorage::default()
        .with_card(CARD)
        .with_file(remote_path(vendor), local_bytes(3000));
    let f = fixture("associate", storage);
    let originals = f.root.join("originals");
    let write = |path: &Path, bytes: usize| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, local_bytes(bytes)).unwrap();
    };
    let same_name = originals.join("trip").join("Clip.mp4");
    let native = originals.join("native-take.mp4");
    write(&same_name, 3000);
    write(&native, 3000);
    write(&originals.join("other.mp4"), 4000);
    write(&originals.join("still.png"), 3000);
    let listed = f.backend.manager_overview(KEY, TIME).unwrap();
    assert_eq!(listed.files[0].entry, None);

    let sources = vec![originals.display().to_string()];
    let found = f
        .backend
        .associate_candidates(KEY, vendor, &sources, TIME)
        .unwrap()
        .candidates;
    let names: Vec<(&str, bool)> = found
        .iter()
        .map(|c| (c.name.as_str(), c.same_name))
        .collect();
    assert_eq!(names, [("Clip.mp4", true), ("native-take.mp4", false)]);
    assert_eq!((found[0].size, found[0].kind), (3000, "video"));
    let native_size = found[1].resolution.unwrap();
    assert_eq!((native_size.width, native_size.height), (480, 1920));

    let source = same_name.display().to_string();
    let err = f
        .backend
        .associate_original(KEY, vendor, &source, Confirm::No, TIME)
        .unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    assert!(record(&f).entries.is_empty());

    let file = f
        .backend
        .associate_original(KEY, vendor, &source, Confirm::Yes, TIME)
        .unwrap();
    let entry = file.entry.clone().unwrap();
    assert_eq!((entry.state, entry.local_copy), ("stored", true));
    assert_eq!(entry.source.as_deref(), Some(source.as_str()));
    assert_eq!((file.file.size, file.finding), (Some(3000), None));
    let saved = record(&f).entries[0].clone();
    assert_eq!(saved.content, content_id(&local_bytes(3000)));
    assert!(catalog(&f).has_copy(&saved.content));
    assert!(f.writes().is_empty(), "only queries");
    // Now it can move.
    let movable = ready_plan(plan_of(&f, &ask_move(&[vendor], "internal")));
    assert!(movable.skipped.is_empty());
    let other = f
        .backend
        .associate_original(
            KEY,
            vendor,
            &originals.join("other.mp4").display().to_string(),
            Confirm::Yes,
            TIME,
        )
        .unwrap_err();
    assert_eq!(other.code(), "invalidInput", "another size");
}

#[test]
fn the_cache_lists_clears_and_limits_copies() {
    let f = fixture("cache", FakeStorage::default());
    let a = send(&f, "native-a.mp4", 3000, "internal");
    send(&f, "native-b.mp4", 2000, "internal");
    f.backend
        .delete_stored(KEY, &a, Confirm::Yes, TIME)
        .unwrap();
    let info = f.backend.cache_info().unwrap();
    assert_eq!(
        (
            info.copies,
            info.bytes,
            info.deleted_copies,
            info.deleted_bytes,
            info.limit
        ),
        (2, 5000, 1, 3000, 2 << 30)
    );
    let json = serde_json::to_value(info).unwrap();
    assert_eq!(json["deletedBytes"], 3000);

    let err = f.backend.clear_cache("deleted", Confirm::No).unwrap_err();
    assert_eq!(err.code(), "notConfirmed");
    assert_eq!(
        f.backend
            .clear_cache("bogus", Confirm::Yes)
            .unwrap_err()
            .code(),
        "invalidInput"
    );
    assert_eq!(f.backend.cache_info().unwrap().copies, 2);
    let cleared = f.backend.clear_cache("deleted", Confirm::Yes).unwrap();
    assert_eq!((cleared.removed, cleared.bytes), (1, 3000));
    assert_eq!(
        serde_json::to_value(cleared).unwrap(),
        json!({"removed": 1, "bytes": 3000})
    );
    assert_eq!(record(&f).entries.len(), 2, "the entries stay");

    assert_eq!(
        f.backend.set_cache_limit(0).unwrap_err().code(),
        "invalidInput"
    );
    let c = send(&f, "native-c.mp4", 1000, "internal");
    f.backend
        .delete_stored(KEY, &c, Confirm::Yes, TIME)
        .unwrap();
    assert_eq!(f.backend.cache_info().unwrap().deleted_copies, 1);
    let limited = f.backend.set_cache_limit(1).unwrap();
    assert_eq!(
        (limited.limit, limited.deleted_copies, limited.copies),
        (1, 0, 1)
    );
    let cleared = f.backend.clear_cache("all", Confirm::Yes).unwrap();
    assert_eq!((cleared.removed, cleared.bytes), (1, 2000));
    assert!(
        f.writes()
            .iter()
            .filter(|c| matches!(c, StorageCall::Delete(_)))
            .count()
            == 2
    );
}

/// The fake Turing USB 8.8": it cannot delete through Bezel.
fn turing_usb(name: &str, storage: FakeStorage) -> Fixture {
    let bus = FakeBus::new(vec![Endpoint {
        address: DeviceAddress("usb:3-1".into()),
        transport: Transport::UsbBulk,
        usb: UsbId::new(0x1cbe, 0x0088),
        serial_number: None,
        manufacturer: None,
        product: None,
        location: None,
    }]);
    let copies = Copies::in_memory(MemoryArchive::new());
    fixture_on(
        &format!("manager-{name}"),
        bus,
        storage,
        FakeMedia::ready(),
        copies,
    )
}

#[test]
fn turing_usb_screens_never_delete_move_or_clean_up() {
    const USB: &str = "usb:3-1";
    let vendor = remote_path("internal/video/vendor.mp4");
    let storage = FakeStorage::default()
        .with_card(CARD)
        .with_file_of_unknown_size(vendor.clone(), vec![1; 500])
        .with_file_of_unknown_size(remote_path("internal/video/vendor.mp4.mp4"), vec![1; 500]);
    let f = turing_usb("turing-usb", storage);
    let ready_upload = match f
        .backend
        .prepare_upload(USB, &f.local("logo.png", 900), "internal", TIME)
        .unwrap()
    {
        crate::dto::PrepareDto::Ready(ready) => ready,
        crate::dto::PrepareDto::Refused(r) => panic!("refused: {r:?}"),
    };
    let mut progress = |_| {};
    let job = f
        .backend
        .run_upload(ready_upload.ticket, Confirm::No, TIME, &mut progress)
        .unwrap();
    assert!(matches!(job, crate::dto::JobDto::Done { .. }), "{job:?}");
    let sent = ready_upload.target.path;

    let dto = f.backend.manager_overview(USB, TIME).unwrap();
    assert!(!dto.deletes);
    assert!(dto.files.iter().all(|f| f.finding.is_none()), "no cleanup");
    let size = |path: &str| {
        dto.files
            .iter()
            .find(|f| f.file.path == path)
            .unwrap()
            .file
            .size
    };
    assert_eq!(size(&sent), Some(900), "the catalog's size");
    assert_eq!(size("internal/video/vendor.mp4"), None, "unknown");

    let writes = f.writes().len();
    let moved = f
        .backend
        .plan_transfer(USB, &ask_move(&[&sent], "sd"), &[], TIME);
    let moved = ready_plan(moved);
    assert_eq!(
        (moved.steps.len(), moved.skipped[0].code),
        (0, "deleteUnsupported")
    );
    let err = f
        .backend
        .run_plan(moved.ticket, Confirm::Yes, TIME, &mut |_| {})
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    let err = f
        .backend
        .delete_files(
            USB,
            std::slice::from_ref(&sent),
            Confirm::Yes,
            TIME,
            &mut |_| {},
        )
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    let err = f
        .backend
        .delete_stored(USB, &sent, Confirm::Yes, TIME)
        .unwrap_err();
    assert_eq!(err.code(), "unsupported");
    assert_eq!(f.writes().len(), writes, "nothing reached the screen");

    // Copying keeps the source: it runs.
    let copy = Ask::Copy {
        paths: vec![sent.clone()],
        to: "sd".into(),
    };
    let copy = ready_plan(f.backend.plan_transfer(USB, &copy, &[], TIME));
    let report = ran(f
        .backend
        .run_plan(copy.ticket, Confirm::Yes, TIME, &mut |_| {})
        .unwrap());
    assert_eq!(report.done[0].target, "sd/image/logo.png");
    assert!(f.storage().files.contains_key(&remote_path(&sent)));
}

/// A real PNG of `side` x `side` pixels.
fn png(side: u32) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(side, side, image::Rgba([10, 20, 30, 255]))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[test]
fn thumbnails_come_from_the_local_copies_also_during_a_job() {
    let store = std::env::temp_dir().join(format!("bezel-manager-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&store);
    let archive = DiskArchive::open(&store).unwrap();
    let storage =
        FakeStorage::default().with_file(remote_path("internal/image/vendor.png"), png(8));
    let f = fixture_on(
        "manager-thumbs",
        FakeBus::turing_88(),
        storage,
        FakeMedia::ready(),
        Copies::on_disk(archive),
    );
    let local: PathBuf = f.root.join("local").join("logo.png");
    std::fs::create_dir_all(local.parent().unwrap()).unwrap();
    std::fs::write(&local, png(400)).unwrap();
    let ready = f.ready(&local, "internal");
    f.run(ready.ticket, Confirm::No).0.unwrap();
    let path = ready.target.path;
    assert_eq!(
        f.backend.manager_thumbnail(KEY, &path),
        None,
        "not listed yet"
    );

    f.backend.manager_overview(KEY, TIME).unwrap();
    let url = f.backend.manager_thumbnail(KEY, &path).unwrap();
    assert!(url.starts_with("data:image/png;base64,"), "{url}");
    assert_eq!(
        f.backend
            .manager_thumbnail(KEY, "internal/image/vendor.png"),
        None
    );
    assert_eq!(f.backend.manager_thumbnail("COM9", &path), None);
    // Kept on disk with the catalog, shared with the CLI.
    assert!(std::fs::read_dir(store.join("thumbs")).unwrap().count() == 1);
    assert_eq!(
        DiskArchive::open(&store)
            .unwrap()
            .load()
            .unwrap()
            .copies
            .len(),
        1
    );

    // During a job (the claim held), thumbnails still come.
    let again = f.ready(&f.local("native.mp4", FAKE_UPLOAD_CHUNK * 2), "internal");
    let mut during = None;
    f.backend
        .run_upload(again.ticket, Confirm::No, TIME, &mut |p| {
            if during.is_none() && p.phase == JobPhase::Upload {
                during = Some(f.backend.manager_thumbnail(KEY, &path));
            }
        })
        .unwrap();
    assert!(during.unwrap().is_some());
    let _ = std::fs::remove_dir_all(&store);
}
