use std::sync::{mpsc, Arc, Mutex};

pub type PickerResult = Result<Vec<String>, String>;
pub struct Slot(Mutex<bool>);
struct State {
    claimed: bool,
    dismissing: bool,
    owns_paths: bool,
    sender: Option<mpsc::Sender<PickerResult>>,
}
#[derive(Clone)]
pub struct Token(Arc<Mutex<State>>);
impl Token {
    pub fn finish_incomplete_dismissal(&self, result: PickerResult) {
        let owns_paths = self.0.lock().map(|state| state.owns_paths).unwrap_or(true);
        if owns_paths {
            if let Ok(paths) = &result {
                discard(paths);
            }
            self.finish(Err("Picker dismissal ended without completion".into()));
        } else {
            self.finish(result);
        }
    }
    pub fn discard_owned(&self, paths: &[String]) {
        if self.0.lock().map(|state| state.owns_paths).unwrap_or(false) {
            discard(paths);
        }
    }
    pub fn is_open(&self) -> bool {
        self.0
            .lock()
            .map(|state| state.sender.is_some())
            .unwrap_or(false)
    }
    pub fn is_dismissing(&self) -> bool {
        self.0.lock().map(|state| state.dismissing).unwrap_or(false)
    }
    pub fn begin_dismissal(&self) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        if state.sender.is_none() {
            return false;
        }
        state.dismissing = true;
        true
    }
    // Claim before importing. Duplicate/stale callbacks must not create files.
    pub fn claim(&self) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        if state.claimed || state.sender.is_none() {
            return false;
        }
        state.claimed = true;
        true
    }
    pub fn finish(&self, result: PickerResult) {
        // Publish while holding the same lock Request::drop acquires before
        // draining. Otherwise a send can race between its drain and receiver drop.
        let abandoned = match self.0.lock() {
            Ok(mut state) => match state.sender.take() {
                Some(sender) => sender.send(result).err().map(|error| error.0),
                None => Some(result),
            },
            Err(_) => Some(result),
        };
        if let Some(Ok(paths)) = abandoned {
            self.discard_owned(&paths);
        }
    }
}

// Only request-owned private import paths may be passed here.
pub fn discard(paths: &[String]) {
    for path in paths {
        let _ = std::fs::remove_file(path);
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}
pub struct Imports(Vec<String>);
impl Imports {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn push(&mut self, path: String) {
        self.0.push(path);
    }
    pub fn take(mut self) -> Vec<String> {
        std::mem::take(&mut self.0)
    }
}
impl Drop for Imports {
    fn drop(&mut self) {
        discard(&self.0);
    }
}
impl Slot {
    pub const fn new() -> Self {
        Self(Mutex::new(false))
    }
    pub fn begin(&self) -> Result<Request<'_>, String> {
        self.begin_with_ownership(true)
    }
    pub fn begin_export(&self) -> Result<Request<'_>, String> {
        self.begin_with_ownership(false)
    }
    fn begin_with_ownership(&self, owns_paths: bool) -> Result<Request<'_>, String> {
        let mut busy = self.0.lock().map_err(|_| "Picker state poisoned")?;
        if *busy {
            return Err("A picker request is already active".into());
        }
        *busy = true;
        let (sender, receiver) = mpsc::channel();
        Ok(Request {
            slot: self,
            receiver,
            token: Token(Arc::new(Mutex::new(State {
                claimed: false,
                dismissing: false,
                owns_paths,
                sender: Some(sender),
            }))),
        })
    }
}
pub struct Request<'a> {
    slot: &'a Slot,
    pub receiver: mpsc::Receiver<PickerResult>,
    pub token: Token,
}
impl Drop for Request<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.token.0.lock() {
            state.sender.take();
        }
        while let Ok(Ok(paths)) = self.receiver.try_recv() {
            self.token.discard_owned(&paths);
        }
        if let Ok(mut busy) = self.slot.0.lock() {
            *busy = false;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_and_duplicate_callbacks_cannot_complete_next_request() {
        let slot = Slot::new();
        let first = slot.begin().unwrap();
        let stale = first.token.clone();
        assert!(slot.begin().is_err());
        assert!(stale.claim());
        stale.finish(Ok(vec![]));
        assert_eq!(first.receiver.recv().unwrap(), Ok(vec![]));
        assert!(!stale.claim());
        drop(first);
        let second = slot.begin().unwrap();
        stale.finish(Err("late A".into()));
        assert!(matches!(
            second.receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(second.token.claim());
        second.token.finish(Err("real B".into()));
        assert_eq!(second.receiver.recv().unwrap(), Err("real B".into()));
    }
    #[test]
    fn abandoned_success_reclaims_private_import() {
        let root = std::env::temp_dir().join(format!("gpui-request-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("import.txt");
        std::fs::write(&file, "owned").unwrap();
        let slot = Slot::new();
        let request = slot.begin().unwrap();
        let token = request.token.clone();
        drop(request);
        assert!(!token.claim());
        token.finish(Ok(vec![file.to_string_lossy().into_owned()]));
        assert!(!root.exists());
        assert!(slot.begin().is_ok());
    }
    #[test]
    fn incomplete_dismissal_preserves_export_and_reclaims_import() {
        let root = std::env::temp_dir().join(format!("gpui-dismissal-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("file.txt");
        std::fs::write(&file, "data").unwrap();
        let path = file.to_string_lossy().into_owned();
        let slot = Slot::new();
        let export = slot.begin_export().unwrap();
        export
            .token
            .finish_incomplete_dismissal(Ok(vec![path.clone()]));
        assert_eq!(export.receiver.recv().unwrap(), Ok(vec![path.clone()]));
        assert!(file.exists());
        drop(export);
        let import = slot.begin().unwrap();
        import.token.finish_incomplete_dismissal(Ok(vec![path]));
        assert!(import.receiver.recv().unwrap().is_err());
        assert!(!root.exists());
    }
    #[test]
    fn abandoned_export_never_deletes_the_destination() {
        let root =
            std::env::temp_dir().join(format!("gpui-export-request-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let file = root.join("export.txt");
        std::fs::write(&file, "user data").unwrap();
        let path = file.to_string_lossy().into_owned();
        let slot = Slot::new();

        let request = slot.begin_export().unwrap();
        let token = request.token.clone();
        assert!(token.claim());
        drop(request);
        token.finish(Ok(vec![path.clone()]));
        assert_eq!(std::fs::read(&file).unwrap(), b"user data");

        let request = slot.begin_export().unwrap();
        assert!(request.token.claim());
        request.token.finish(Ok(vec![path.clone()]));
        drop(request);
        assert_eq!(std::fs::read(&file).unwrap(), b"user data");

        let request = slot.begin_export().unwrap();
        request.token.discard_owned(&[path]);
        drop(request);
        assert_eq!(std::fs::read(&file).unwrap(), b"user data");
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn partial_import_failure_and_duplicate_success_reclaim_files() {
        let root =
            std::env::temp_dir().join(format!("gpui-partial-import-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("first.txt");
        std::fs::write(&path, "owned").unwrap();
        {
            let mut imports = Imports::new();
            imports.push(path.to_string_lossy().into_owned());
            // A later malformed URL exits before transferring ownership.
        }
        assert!(!root.exists());
        std::fs::create_dir(&root).unwrap();
        std::fs::write(&path, "late").unwrap();
        let slot = Slot::new();
        let request = slot.begin().unwrap();
        assert!(request.token.claim());
        request.token.finish(Ok(vec![]));
        request
            .token
            .finish(Ok(vec![path.to_string_lossy().into_owned()]));
        assert_eq!(request.receiver.recv().unwrap(), Ok(vec![]));
        assert!(!root.exists());
    }
    #[test]
    fn native_teardown_ends_claimed_import_and_dropped_provider_queue_cleans_up() {
        let slot = Slot::new();
        let request = slot.begin().unwrap();
        assert!(request.token.claim());
        assert!(request.token.is_open());
        assert!(!request.token.is_dismissing());
        request
            .token
            .finish(Err("Controller destroyed during import".into()));
        assert!(!request.token.is_open());
        assert!(!request.token.begin_dismissal());
        assert!(request.receiver.recv().unwrap().is_err());

        let root =
            std::env::temp_dir().join(format!("gpui-provider-queue-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("queued.txt");
        std::fs::write(&path, "owned").unwrap();
        let mut imports = Imports::new();
        imports.push(path.to_string_lossy().into_owned());
        let (sender, receiver) = mpsc::channel();
        assert!(sender.send(imports).is_ok());
        drop(receiver); // Cancellation raced with a successful provider send.
        assert!(!root.exists());
    }
}
