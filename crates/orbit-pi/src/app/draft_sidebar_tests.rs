//! Sidebar drafts: a New Task row appears immediately, an empty one
//! disappears on leave, and typed text (or an attachment) stays with the row.

use super::*;
use crate::theme::{Theme, ThemeKey};

/// A process that stays alive long enough for New Task to reuse it, without
/// launching a real pi. Dropped after the app releases the client.
struct KeepAlive {
    dir: PathBuf,
}

impl KeepAlive {
    fn start(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "orbit-draft-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("fake pi dir");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bin = dir.join("fake-pi");
            std::fs::write(&bin, "#!/bin/sh\nexec cat >/dev/null\n").expect("fake pi");
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        #[cfg(windows)]
        {
            std::fs::write(
                dir.join("fake-pi.cmd"),
                "@echo off\r\nping -n 40 127.0.0.1 > nul\r\n",
            )
            .expect("fake pi");
        }
        Self { dir }
    }

    fn bin(&self) -> PathBuf {
        #[cfg(unix)]
        {
            self.dir.join("fake-pi")
        }
        #[cfg(windows)]
        {
            self.dir.join("fake-pi.cmd")
        }
    }
}

impl Drop for KeepAlive {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn prepare(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeKey::ORBIT)));
}

fn adopt_alive(app: &mut OrbitApp, fake: &KeepAlive) {
    let workspace = app
        .current_workspace
        .clone()
        .unwrap_or_else(|| std::env::temp_dir());
    let client =
        PiClient::spawn_with_bin(fake.bin().to_str().expect("utf-8 path"), &workspace, None)
            .expect("spawn keep-alive");
    app.adopt_client(client);
}

fn info(path: &str, cwd: &Path) -> SessionInfo {
    SessionInfo {
        path: PathBuf::from(path),
        id: "session".into(),
        cwd: cwd.to_path_buf(),
        title: "existing".into(),
        first_message: "hello".into(),
        modified: SystemTime::UNIX_EPOCH,
        draft: None,
    }
}

fn missing_session(name: &str) -> SessionInfo {
    SessionInfo {
        path: PathBuf::from(format!("/orbit-missing/{name}.jsonl")),
        id: name.into(),
        cwd: PathBuf::from("/orbit-missing-cwd"),
        title: name.into(),
        first_message: String::new(),
        modified: SystemTime::UNIX_EPOCH,
        draft: None,
    }
}

fn blank_draft(id: u64, cwd: &Path) -> ListedDraft {
    ListedDraft {
        id,
        path: drafts::draft_placeholder(id),
        bound: false,
        cwd: cwd.to_path_buf(),
        created: SystemTime::UNIX_EPOCH + Duration::from_secs(id),
        composer: None,
        promoted: false,
        sent_title: None,
    }
}

/// Reuse saves the session being left, clears the new composer, and a click
/// that lands before pi names the file cannot write that new text onto the
/// old session.
#[gpui::test]
fn reuse_keeps_old_text_and_a_typed_draft_survives_leaving(cx: &mut gpui::TestAppContext) {
    let fake = KeepAlive::start("reuse");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            let old = info("old-session.jsonl", &cwd);
            app.sessions.push(old.clone());
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd);
            app.current_session_path = Some(old.path.clone());
            adopt_alive(app, &fake);
            app.input
                .update(cx, |input, cx| input.set_text("keep me", cx));

            app.on_new_session(&crate::NewSession, window, cx);

            assert!(app.current_session_path.is_none());
            assert!(app.current_draft_id.is_some());
            assert_eq!(app.input.read(cx).text(), "");
            assert_eq!(
                app.drafts.get(&old.path).map(|draft| draft.text.as_str()),
                Some("keep me")
            );
            assert_eq!(app.listed_drafts.len(), 1);
            let id = app.current_draft_id;

            app.on_new_session(&crate::NewSession, window, cx);
            assert_eq!(app.listed_drafts.len(), 1, "a blank draft is not stacked");
            assert_eq!(app.current_draft_id, id);

            app.input
                .update(cx, |input, cx| input.set_text("fresh", cx));
            app.files.push(AttachedFile {
                path: PathBuf::from("notes.md"),
                name: "notes.md".into(),
                size: 4,
            });
            app.switch_to_session(missing_session("other"), false, cx);

            assert_eq!(
                app.drafts.get(&old.path).map(|draft| draft.text.as_str()),
                Some("keep me"),
                "text typed on the new row must not land on the old session"
            );
            assert_eq!(app.listed_drafts.len(), 1);
            let kept = app.listed_drafts[0].composer.as_ref().expect("kept");
            assert_eq!(kept.text, "fresh");
            assert_eq!(kept.files.len(), 1);
            assert_eq!(app.pending_parks.len(), 1);
            assert!(!app.pending_parks[0].discard);
            assert_eq!(app.pending_parks[0].draft, id);

            let parked = app.pending_parks.pop().expect("park");
            let file = std::env::temp_dir().join(format!(
                "orbit-draft-rekey-{}-{}.jsonl",
                std::process::id(),
                id.unwrap_or(0)
            ));
            app.admit_rekeyed_park(file.clone(), parked);
            assert!(app.lives.contains_key(&file));
            assert!(app.listed_drafts[0].bound);
            assert_eq!(app.listed_drafts[0].path, file);
            app.lives.remove(&file);
            let _ = std::fs::remove_file(&file);
        });
    });
}

/// Leaving before the file is named drops an empty draft and, once the file
/// arrives, deletes it only when it is still header-only.
#[gpui::test]
fn an_empty_draft_is_discarded_and_its_header_file_is_removed(cx: &mut gpui::TestAppContext) {
    let fake = KeepAlive::start("discard");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            let old = info("old-session.jsonl", &cwd);
            app.sessions.push(old.clone());
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd);
            app.current_session_path = Some(old.path.clone());
            adopt_alive(app, &fake);
            app.input.update(cx, |input, cx| input.set_text("stay", cx));
            app.on_new_session(&crate::NewSession, window, cx);
            app.switch_to_session(missing_session("elsewhere"), false, cx);

            assert!(app.listed_drafts.is_empty());
            assert_eq!(
                app.drafts.get(&old.path).map(|draft| draft.text.as_str()),
                Some("stay")
            );
            assert_eq!(app.pending_parks.len(), 1);
            assert!(app.pending_parks[0].discard);

            let file = std::env::temp_dir()
                .join(format!("orbit-draft-header-{}.jsonl", std::process::id()));
            std::fs::write(
                &file,
                "{\"type\":\"session\",\"id\":\"draft\",\"cwd\":\"/tmp\"}\n",
            )
            .unwrap();
            let parked = app.pending_parks.pop().expect("park");
            app.admit_rekeyed_park(file.clone(), parked);
            assert!(!file.exists(), "a header-only new-task file is removed");
            assert!(app.lives.is_empty());
        });
    });
}

#[gpui::test]
fn whitespace_is_empty_and_an_attachment_is_kept(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.next_draft_id = 3;
            app.listed_drafts.push(blank_draft(3, &cwd));
            app.current_draft_id = Some(3);
            app.input
                .update(cx, |input, cx| input.set_text("   \n", cx));
            app.switch_to_session(missing_session("blank"), false, cx);
            assert!(app.listed_drafts.is_empty(), "whitespace is an empty draft");

            app.next_draft_id = 4;
            app.listed_drafts.push(blank_draft(4, &cwd));
            app.current_draft_id = Some(4);
            app.current_session_path = None;
            app.files.push(AttachedFile {
                path: PathBuf::from("shot.png"),
                name: "shot.png".into(),
                size: 8,
            });
            app.switch_to_session(missing_session("kept"), false, cx);
            assert_eq!(app.listed_drafts.len(), 1);
            assert_eq!(
                app.listed_drafts[0].composer.as_ref().unwrap().files.len(),
                1
            );
        });
    });
}

#[gpui::test]
fn history_back_does_not_open_a_discarded_draft(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.next_draft_id = 8;
            app.listed_drafts.push(blank_draft(8, &cwd));
            app.current_draft_id = Some(8);
            let draft = app.session_info_from_draft(&app.listed_drafts[0]);
            let earlier = missing_session("earlier");
            app.session_history = vec![earlier, draft.clone()];
            app.history_index = 1;

            app.switch_to_session(missing_session("next"), false, cx);

            assert!(app.listed_drafts.is_empty());
            assert!(app
                .session_history
                .iter()
                .all(|session| session.draft.is_none()));
            let stayed = app.current_session_path.clone();
            app.switch_to_session(draft, false, cx);
            assert_eq!(app.current_session_path, stayed);
            assert!(app.current_draft_id.is_none());
        });
    });
}

#[gpui::test]
fn one_list_includes_the_draft_row(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            let cwd = PathBuf::from("/work/alpha");
            app.workspaces.push(cwd.clone());
            app.sidebar_group_by = SidebarGroupBy::OneList;
            app.next_draft_id = 2;
            app.listed_drafts.push(blank_draft(2, &cwd));
            app.listed_drafts[0].created = SystemTime::now() + Duration::from_secs(86_400);
            app.current_draft_id = Some(2);
            let sessions = app.sidebar_sessions();
            let ix = sessions
                .iter()
                .position(|session| session.draft == Some(2))
                .expect("draft row");
            assert_eq!(ix, 0, "a just-created draft leads the flat list");
            let rows = app.sidebar_rows(&sessions, &HashSet::new());
            assert!(rows
                .iter()
                .any(|row| matches!(row, SideRow::Session(index) if *index == ix)));
        });
    });
}

/// Once the disk scan lists a sent draft's file, that in-memory row is gone.
/// An unsent draft with its own path stays.
#[gpui::test]
fn a_sent_draft_drops_out_when_its_file_is_in_the_scan(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, _cx| {
            let cwd = std::env::temp_dir();
            let path = cwd.join("promoted-draft.jsonl");
            app.sessions.push(info(path.to_str().expect("utf-8"), &cwd));
            app.current_session_path = Some(path.clone());
            app.current_draft_id = Some(9);
            app.listed_drafts.push(ListedDraft {
                id: 9,
                path: path.clone(),
                bound: true,
                cwd: cwd.clone(),
                created: SystemTime::UNIX_EPOCH,
                composer: Some(ComposerDraft {
                    text: "later".into(),
                    attachments: Vec::new(),
                    files: Vec::new(),
                    pasted_texts: Vec::new(),
                }),
                promoted: true,
                sent_title: Some("sent".into()),
            });
            let other = cwd.join("still-a-draft.jsonl");
            app.sessions
                .push(info(other.to_str().expect("utf-8"), &cwd));
            app.listed_drafts.push(ListedDraft {
                id: 10,
                path: other,
                bound: true,
                cwd,
                created: SystemTime::UNIX_EPOCH,
                composer: None,
                promoted: false,
                sent_title: None,
            });

            app.release_promoted_drafts_on_disk();

            assert!(app.listed_drafts.iter().all(|draft| draft.id != 9));
            assert_eq!(app.listed_drafts.len(), 1);
            assert!(app.current_draft_id.is_none());
            assert_eq!(
                app.drafts.get(&path).map(|draft| draft.text.as_str()),
                Some("later")
            );
        });
    });
}

/// Deleting the open draft clears the run state the row was showing.
#[gpui::test]
fn deleting_the_open_draft_clears_its_run_state(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.listed_drafts.push(blank_draft(3, &cwd));
            app.current_draft_id = Some(3);
            app.busy = true;
            app.current_title = Some("leak".into());
            app.session_id = Some("sess".into());

            app.delete_listed_draft(3, cx);

            assert!(app.listed_drafts.is_empty());
            assert!(!app.busy);
            assert!(app.current_title.is_none());
            assert!(app.session_id.is_none());
            assert!(app.current_draft_id.is_none());
        });
    });
}

/// A blank page with no workspace still registers under the real folder.
#[gpui::test]
fn an_unlisted_page_keeps_the_task_folder(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_workspace = None;
            app.current_session_path = None;
            app.current_draft_id = None;
            app.input.update(cx, |input, cx| input.set_text("kept", cx));
            app.save_composer_draft(cx);
            assert_eq!(app.listed_drafts.len(), 1);
            assert_eq!(
                app.listed_drafts[0].cwd,
                std::env::current_dir().expect("cwd")
            );
        });
    });
}

/// Opening a pending draft restores that process's counts and drops the
/// title of the view that was on screen.
#[gpui::test]
fn reopening_a_pending_draft_does_not_keep_the_other_title(cx: &mut gpui::TestAppContext) {
    let fake = KeepAlive::start("pending-resume");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.next_draft_id = 5;
            app.listed_drafts.push(blank_draft(5, &cwd));
            app.current_draft_id = Some(5);
            app.current_session_path = None;
            app.added = 7;
            adopt_alive(app, &fake);
            app.parking_draft = Some(5);
            app.park_active_session();

            app.current_title = Some("other view".into());
            app.session_id = Some("other".into());
            app.added = 99;
            app.current_draft_id = None;
            let info = app.session_info_from_draft(&app.listed_drafts[0]);

            app.switch_to_session(info, false, cx);

            assert_eq!(app.current_draft_id, Some(5));
            assert!(app.current_title.is_none());
            assert!(app.session_id.is_none());
            assert_eq!(app.added, 7);
            app.drop_client();
        });
    });
}

/// Choosing the folder the blank row already uses focuses that row.
#[gpui::test]
fn starting_in_the_same_folder_does_not_stack_a_blank_row(cx: &mut gpui::TestAppContext) {
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            app.next_draft_id = 1;
            app.listed_drafts.push(blank_draft(1, &cwd));
            app.current_draft_id = Some(1);

            app.start_task_in_folder(cwd, window, cx);

            assert_eq!(app.listed_drafts.len(), 1);
            assert_eq!(app.current_draft_id, Some(1));
        });
    });
}

/// A draft whose process is still warm comes back as that process.
#[gpui::test]
fn a_warm_draft_resumes_without_switching_session(cx: &mut gpui::TestAppContext) {
    let fake = KeepAlive::start("lives-resume");
    prepare(cx);
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let cwd = std::env::temp_dir();
            let path = cwd.join("warm-draft.jsonl");
            app.workspaces.push(cwd.clone());
            app.current_workspace = Some(cwd.clone());
            adopt_alive(app, &fake);
            let pid = app.client.as_ref().map(PiClient::child_pid);
            app.current_session_path = Some(path.clone());
            app.park_active_session();
            assert!(app.client.is_none());

            app.next_draft_id = 6;
            app.listed_drafts.push(ListedDraft {
                id: 6,
                path: path.clone(),
                bound: true,
                cwd,
                created: SystemTime::UNIX_EPOCH,
                composer: Some(ComposerDraft {
                    text: "still here".into(),
                    attachments: Vec::new(),
                    files: Vec::new(),
                    pasted_texts: Vec::new(),
                }),
                promoted: false,
                sent_title: None,
            });
            app.current_draft_id = None;
            let info = app.session_info_from_draft(&app.listed_drafts[0]);
            app.switch_to_session(info, false, cx);

            assert_eq!(app.current_draft_id, Some(6));
            assert_eq!(app.current_session_path.as_ref(), Some(&path));
            assert_eq!(app.client.as_ref().map(PiClient::child_pid), pid);
            assert!(!app.lives.contains_key(&path));
            assert_eq!(app.input.read(cx).text(), "still here");
            app.drop_client();
        });
    });
}
