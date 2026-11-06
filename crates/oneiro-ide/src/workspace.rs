//! The open project: documents being edited, the compiled episode, diagnostics and the lock file.

use oneiro_core::lock::{self, Lock, LockPlace, LockShot};
use oneiro_core::math::V2;
#[cfg(test)]
use oneiro_core::math::dist2;
use oneiro_core::project::locate;
use oneiro_core::{compile_source, CameraPose, Episode, Project};
use oneiro_lang::Diagnostic;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocKind {
    Script,
    Set,
    Character,
    Other,
}

pub struct Doc {
    pub path: PathBuf,
    pub text: String,
    pub saved: String,
    pub kind: DocKind,
}

impl Doc {
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }
    pub fn title(&self) -> String {
        self.path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    }
}

pub fn kind_of(path: &Path) -> DocKind {
    match path.extension().and_then(|e| e.to_str()) {
        Some("onr") => DocKind::Script,
        Some("onset") => DocKind::Set,
        Some("onchar") => DocKind::Character,
        _ => DocKind::Other,
    }
}

pub struct Workspace {
    pub root: PathBuf,
    pub project: Project,
    pub docs: Vec<Doc>,
    pub active: usize,
    /// Index into `docs` of the script that is compiled and shown.
    pub script: usize,
    pub lock: Lock,
    pub lock_path: PathBuf,
    pub episode: Arc<Episode>,
    pub diags: Vec<Diagnostic>,
    pub project_diags: Vec<Diagnostic>,
    pub valid: bool,
    pub last_edit: Option<Instant>,
    pub generation: u64,
    /// Modification time of the script on disk when we last read or wrote it.
    pub disk_mtime: Option<std::time::SystemTime>,
    pub last_disk_check: Instant,
    /// The script changed on disk while there are unsaved edits in the editor.
    pub external_conflict: bool,
}

impl Workspace {
    pub fn open(path: &Path) -> Result<Workspace, String> {
        let (root, script) = locate(path)?;
        let mut project_diags = Vec::new();
        let project = Project::load(&root, &mut project_diags);
        let text = std::fs::read_to_string(&script).map_err(|e| format!("{}: {}", script.display(), e))?;
        let lock_path = script.with_extension("onlock");
        let lock = Lock::load(&lock_path);
        let empty = Episode {
            name: project.manifest.name.clone(),
            fps: project.manifest.fps,
            width: project.manifest.width,
            height: project.manifest.height,
            scenes: vec![],
            duration: 0.0,
            audio: vec![],
            duck_db: -9.0,
            state: Default::default(),
            root: root.clone(),
        };
        let mut ws = Workspace {
            root,
            project,
            docs: vec![Doc { path: script.clone(), saved: text.clone(), text, kind: DocKind::Script }],
            active: 0,
            script: 0,
            lock,
            lock_path,
            episode: Arc::new(empty),
            diags: vec![],
            project_diags,
            valid: false,
            last_edit: None,
            generation: 0,
            disk_mtime: std::fs::metadata(&script).and_then(|m| m.modified()).ok(),
            last_disk_check: Instant::now(),
            external_conflict: false,
        };
        ws.recompile();
        Ok(ws)
    }

    pub fn script_doc(&self) -> &Doc {
        &self.docs[self.script]
    }

    pub fn label(&self) -> String {
        self.script_doc().path.strip_prefix(&self.root).unwrap_or(&self.script_doc().path).to_string_lossy().replace('\\', "/")
    }

    pub fn recompile(&mut self) {
        let label = self.label();
        let text = self.docs[self.script].text.clone();
        let (ep, d) = compile_source(&self.project, &label, &text, &self.lock);
        self.valid = !d.iter().any(|x| x.is_error());
        let mut all = self.project_diags.clone();
        all.extend(d);
        self.diags = all;
        if self.valid || self.episode.scenes.is_empty() {
            self.episode = Arc::new(ep);
        }
        self.generation += 1;
        self.last_edit = None;
    }

    /// Detects edits made outside the IDE (an AI agent through MCP, another editor). When the editor
    /// has no unsaved changes the new text is loaded and recompiled; otherwise a conflict is flagged.
    /// Returns a message when something changed. Checks at most once per second unless `force`.
    pub fn check_external(&mut self, force: bool) -> Option<String> {
        if !force && self.last_disk_check.elapsed() < std::time::Duration::from_millis(1000) {
            return None;
        }
        self.last_disk_check = Instant::now();
        let path = self.docs[self.script].path.clone();
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
        if Some(mtime) == self.disk_mtime {
            return None;
        }
        self.disk_mtime = Some(mtime);
        let text = std::fs::read_to_string(&path).ok()?;
        let idx = self.script;
        if text == self.docs[idx].text {
            self.docs[idx].saved = text;
            return None;
        }
        if self.docs[idx].dirty() {
            self.external_conflict = true;
            return Some("The script changed on disk while you have unsaved edits. Save to overwrite it, or reopen the project to load the new version.".into());
        }
        self.docs[idx].text = text.clone();
        self.docs[idx].saved = text;
        self.recompile();
        Some("The script was changed outside the IDE (an AI agent or another editor) and has been reloaded.".into())
    }

    pub fn reload_project(&mut self) {
        let mut d = Vec::new();
        self.project = Project::load(&self.root, &mut d);
        self.project_diags = d;
    }

    pub fn open_doc(&mut self, path: &Path) -> Result<usize, String> {
        if let Some(i) = self.docs.iter().position(|d| d.path == path) {
            return Ok(i);
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        self.docs.push(Doc { path: path.to_path_buf(), saved: text.clone(), text, kind: kind_of(path) });
        Ok(self.docs.len() - 1)
    }

    /// Switches the compiled script to the document at `idx` (must be a script).
    pub fn set_script(&mut self, idx: usize) {
        if self.docs[idx].kind == DocKind::Script {
            self.script = idx;
            self.lock_path = self.docs[idx].path.with_extension("onlock");
            self.disk_mtime = std::fs::metadata(&self.docs[idx].path).and_then(|m| m.modified()).ok();
            self.external_conflict = false;
            self.lock = Lock::load(&self.lock_path);
            self.episode = Arc::new(Episode {
                name: self.project.manifest.name.clone(),
                fps: self.project.manifest.fps,
                width: self.project.manifest.width,
                height: self.project.manifest.height,
                scenes: vec![],
                duration: 0.0,
                audio: vec![],
                duck_db: -9.0,
                state: Default::default(),
                root: self.root.clone(),
            });
            self.recompile();
        }
    }

    pub fn close_doc(&mut self, idx: usize) {
        if self.docs.len() <= 1 || idx == self.script {
            return;
        }
        self.docs.remove(idx);
        if self.script > idx {
            self.script -= 1;
        }
        if self.active >= self.docs.len() {
            self.active = self.docs.len() - 1;
        } else if self.active > idx {
            self.active -= 1;
        }
    }

    pub fn save_doc(&mut self, idx: usize) -> Result<(), String> {
        let path = self.docs[idx].path.clone();
        std::fs::write(&path, &self.docs[idx].text).map_err(|e| format!("{}: {}", path.display(), e))?;
        self.docs[idx].saved = self.docs[idx].text.clone();
        if idx == self.script {
            self.disk_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            self.external_conflict = false;
        }
        if self.docs[idx].kind != DocKind::Script {
            self.reload_project();
        }
        self.recompile();
        if self.docs[idx].kind == DocKind::Script {
            self.write_lock();
        }
        Ok(())
    }

    pub fn save_all(&mut self) -> Result<usize, String> {
        let dirty: Vec<usize> = (0..self.docs.len()).filter(|i| self.docs[*i].dirty()).collect();
        for i in &dirty {
            self.save_doc(*i)?;
        }
        Ok(dirty.len())
    }

    pub fn write_lock(&self) {
        if !self.valid {
            return;
        }
        let name = self.script_doc().title();
        let text = lock::render(&self.episode.scenes, &self.lock, &name);
        if std::fs::read_to_string(&self.lock_path).map_or(true, |old| old != text) {
            let _ = std::fs::write(&self.lock_path, text);
        }
    }

    // ------------------------------------------------------------------ pins

    pub fn pin_shot(&mut self, scene: u32, index: u32, pose: CameraPose, source: String) {
        self.lock.shots.insert((scene, index), LockShot { source, pose, pinned: true });
        self.recompile();
        self.write_lock();
    }

    pub fn unpin_shot(&mut self, scene: u32, index: u32) {
        self.lock.shots.remove(&(scene, index));
        self.recompile();
        self.write_lock();
    }

    pub fn pin_place(&mut self, scene: u32, name: &str, pos: V2, yaw: f64) {
        self.lock.places.insert((scene, name.to_string()), LockPlace { pos, yaw, pinned: true });
        self.recompile();
        self.write_lock();
    }

    pub fn unpin_place(&mut self, scene: u32, name: &str) {
        self.lock.places.remove(&(scene, name.to_string()));
        self.recompile();
        self.write_lock();
    }

    pub fn is_shot_pinned(&self, scene: u32, index: u32) -> bool {
        self.lock.shots.get(&(scene, index)).map_or(false, |s| s.pinned)
    }

    pub fn is_place_pinned(&self, scene: u32, name: &str) -> bool {
        self.lock.places.get(&(scene, name.to_string())).map_or(false, |p| p.pinned)
    }

    // ------------------------------------------------------------------ library

    pub fn scripts(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.root) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map_or(false, |x| x == "onr") {
                    out.push(p);
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(self.root.join("episodes")) {
            for e in rd.flatten() {
                let s = e.path().join("script.onr");
                if s.exists() {
                    out.push(s);
                }
            }
        }
        out.sort();
        out
    }

    pub fn sounds(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for sub in ["library/sounds", "library/music", "audio"] {
            if let Ok(rd) = std::fs::read_dir(self.root.join(sub)) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().map_or(false, |x| x == "wav") {
                        out.push(p);
                    }
                }
            }
        }
        out.sort();
        out
    }

    pub fn character_path(&self, name: &str) -> PathBuf {
        self.root.join("library").join("characters").join(format!("{}.onchar", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{emit, Sample};
    use crate::textops::{insert_lines_after, statement_end_line};

    fn example() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap()
    }

    #[test]
    fn opening_the_example_gives_a_valid_episode() {
        let ws = Workspace::open(&example()).unwrap();
        assert!(ws.valid, "{:?}", ws.diags.iter().map(|d| d.render()).collect::<Vec<_>>());
        assert_eq!(ws.episode.scenes.len(), 1);
        assert!(ws.scripts().len() >= 1 && ws.sounds().len() >= 5);
    }

    #[test]
    fn recorded_motion_inserts_as_valid_script_lines_and_plays_back() {
        let mut ws = Workspace::open(&example()).unwrap();
        let before = ws.episode.duration;
        let text = ws.docs[ws.script].text.clone();
        let line = text.lines().position(|l| l.trim() == "wait 2.5s").unwrap() as u32 + 1;
        assert_eq!(statement_end_line(&text, line), line);

        // A recorded path: 2 m east, then 2 m south (away from Marta, who would otherwise push the target).
        let mut samples = vec![];
        for i in 0..=60 {
            samples.push(Sample { t: i as f64 / 30.0, pos: [-1.0 + 2.0 * i as f64 / 60.0, -0.5], running: false });
        }
        for i in 1..=60 {
            samples.push(Sample { t: (60 + i) as f64 / 30.0, pos: [1.0, -0.5 - 2.0 * i as f64 / 60.0], running: false });
        }
        let lines = emit("Blob", &samples);
        assert_eq!(lines.len(), 2, "{:?}", lines);

        let (new_text, _) = insert_lines_after(&text, line, &lines);
        let idx = ws.script;
        ws.docs[idx].text = new_text;
        ws.recompile();
        assert!(ws.valid, "{:?}", ws.diags.iter().map(|d| d.render()).collect::<Vec<_>>());
        assert!(ws.episode.duration > before + 2.0);
        let blob = ws.episode.scenes[0].actors.iter().find(|a| a.name == "Blob").unwrap();
        assert!(blob.moves.len() >= 3, "recorded moves + the original walk");
        assert!(dist2(blob.moves[0].to, [1.0, -0.5]) < 0.02);
        assert!(dist2(blob.moves[1].to, [1.0, -2.5]) < 0.02);
    }

    #[test]
    fn pinning_a_placement_and_a_shot_survive_recompiles_and_round_trip_to_disk() {
        let tmp = std::env::temp_dir().join(format!("oneiro_ide_ws_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        fn copy_dir(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap().flatten() {
                let p = e.path();
                let t = to.join(e.file_name());
                if p.is_dir() {
                    copy_dir(&p, &t);
                } else {
                    std::fs::copy(&p, &t).unwrap();
                }
            }
        }
        copy_dir(&example(), &tmp);
        let _ = std::fs::remove_file(tmp.join("script.onlock"));
        let mut ws = Workspace::open(&tmp).unwrap();
        ws.pin_place(1, "Blob", [-2.0, 0.5], 1.0);
        assert!(ws.valid && ws.is_place_pinned(1, "Blob"));
        let blob = ws.episode.scenes[0].actors.iter().find(|a| a.name == "Blob").unwrap();
        assert!(dist2(blob.init_pos, [-2.0, 0.5]) < 1e-9);

        let seg = ws.episode.scenes[0].cameras.iter().find(|c| c.index == 2).unwrap().clone();
        let mut pose = seg.base;
        pose.pos[1] = 2.25;
        ws.pin_shot(1, 2, pose, seg.source.clone());
        assert!(ws.is_shot_pinned(1, 2));

        // Reopen from disk: the lock file carries the pins.
        let ws2 = Workspace::open(&tmp).unwrap();
        assert!(ws2.is_place_pinned(1, "Blob") && ws2.is_shot_pinned(1, 2));
        let seg2 = ws2.episode.scenes[0].cameras.iter().find(|c| c.index == 2).unwrap();
        assert!((seg2.base.pos[1] - 2.25).abs() < 1e-3);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod external_tests {
    use super::*;

    fn temp_copy() -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("oneiro_ide_ext_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        fn copy_dir(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for e in std::fs::read_dir(from).unwrap().flatten() {
                let p = e.path();
                let t = to.join(e.file_name());
                if p.is_dir() {
                    copy_dir(&p, &t);
                } else {
                    std::fs::copy(&p, &t).unwrap();
                }
            }
        }
        copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap(), &tmp);
        tmp
    }

    #[test]
    fn edits_made_by_an_agent_on_disk_are_picked_up_or_flagged() {
        let tmp = temp_copy();
        let mut ws = Workspace::open(&tmp).unwrap();
        assert!(ws.check_external(true).is_none());
        let script = tmp.join("script.onr");
        let new_text = ws.docs[0].text.replacen("wait 3.2s", "wait 5s", 1);
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(&script, &new_text).unwrap();
        let msg = ws.check_external(true).expect("external change should be reported");
        assert!(msg.contains("reloaded"), "{}", msg);
        assert!(ws.docs[0].text.contains("wait 5s") && ws.valid && !ws.docs[0].dirty());
        assert!(ws.check_external(true).is_none(), "the same change is not reported twice");

        // With unsaved edits, the IDE must not overwrite them: it flags a conflict instead.
        ws.docs[0].text.push_str("\n# my unsaved note\n");
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(&script, new_text.replacen("wait 5s", "wait 6s", 1)).unwrap();
        let msg = ws.check_external(true).expect("conflict should be reported");
        assert!(msg.contains("unsaved") && ws.external_conflict);
        assert!(ws.docs[0].text.contains("my unsaved note"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
