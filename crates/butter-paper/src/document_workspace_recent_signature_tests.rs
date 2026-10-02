use super::*;
use crate::recent_signature_store::{SignatureKeyStore, SignatureKeyStoreError};
use gpui::TestAppContext;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use zeroize::Zeroizing;

#[derive(Default)]
struct Keys {
    secret: Mutex<Option<Vec<u8>>>,
    mode: AtomicUsize,
    reads: AtomicUsize,
}
impl SignatureKeyStore for Keys {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SignatureKeyStoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        match self.mode.load(Ordering::SeqCst) {
            1 => Err(SignatureKeyStoreError::Unavailable),
            2 => Err(SignatureKeyStoreError::Failure),
            _ => Ok(self.secret.lock().unwrap().clone().map(Zeroizing::new)),
        }
    }
    fn save(&self, key: &[u8]) -> Result<(), SignatureKeyStoreError> {
        if self.mode.load(Ordering::SeqCst) == 3 {
            return Err(SignatureKeyStoreError::Failure);
        }
        *self.secret.lock().unwrap() = Some(key.to_vec());
        Ok(())
    }
}
struct Fixture {
    root: PathBuf,
    keys: Arc<Keys>,
    store: Arc<RecentSignatureStore>,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("bp-recent-{name}-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let keys = Arc::new(Keys::default());
        let store = Arc::new(RecentSignatureStore::new(
            root.join("recent.enc"),
            keys.clone(),
        ));
        Self { root, keys, store }
    }
    fn workspace(&self, cx: &mut TestAppContext) -> Entity<DocumentWorkspace> {
        cx.update(gpui_component::init);
        cx.new(|cx| {
            let mut view = DocumentWorkspace::new(cx);
            view.bind_recent_signature_store(self.store.clone());
            view.signature_popover_open = true;
            view
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn asset(value: u8) -> crate::annotation_model::DecodedRgbaAsset {
    crate::annotation_model::DecodedRgbaAsset::new(2, 1, vec![value, 0, 0, 255, 0, 0, 0, 255])
        .unwrap()
}

#[gpui::test]
fn available_empty_load_is_background_and_distinct_from_unavailable(cx: &mut TestAppContext) {
    let fixture = Fixture::new("empty");
    let view = fixture.workspace(cx);
    view.update(cx, |view, cx| {
        view.load_recent_signatures(cx);
        assert!(view.recent_signatures_loading);
        assert_eq!(fixture.keys.reads.load(Ordering::SeqCst), 0);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.recent_signatures_loading);
        assert!(view.recent_signatures.is_empty());
        assert!(view.recent_signature_storage_issue.is_none());
    });
    fixture.keys.mode.store(1, Ordering::SeqCst);
    view.update(cx, |view, cx| view.load_recent_signatures(cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.recent_signature_storage_issue.as_deref(),
            Some("Recent signatures need secure system storage.")
        )
    });
}

#[gpui::test]
fn successful_load_and_reuse_keep_newest_first_even_when_reopened_immediately(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new("order");
    let view = fixture.workspace(cx);
    let first = asset(1);
    let second = asset(2);
    fixture
        .store
        .remember(first.clone(), RecentSignatureSource::Drawn, 1)
        .unwrap();
    fixture
        .store
        .remember(second.clone(), RecentSignatureSource::Typed, 2)
        .unwrap();
    view.update(cx, |view, cx| view.load_recent_signatures(cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.recent_signatures[0].signature.asset(), &second)
    });
    view.update(cx, |view, cx| {
        view.remember_recent_signature(first.clone(), RecentSignatureSource::Drawn, cx);
        view.load_recent_signatures(cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.recent_signatures.len(), 2);
        assert_eq!(view.recent_signatures[0].signature.asset(), &first);
        assert_eq!(view.recent_signatures[1].signature.asset(), &second);
        assert!(!view.recent_signatures_loading);
    });
}

#[gpui::test]
fn load_save_delete_failures_are_distinct_and_do_not_leave_loading(cx: &mut TestAppContext) {
    let fixture = Fixture::new("failures");
    let view = fixture.workspace(cx);
    fixture.keys.mode.store(2, Ordering::SeqCst);
    view.update(cx, |view, cx| view.load_recent_signatures(cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.recent_signature_storage_issue.as_deref(),
            Some("Recent signatures could not be loaded.")
        )
    });
    fixture.keys.mode.store(3, Ordering::SeqCst);
    view.update(cx, |view, cx| {
        view.remember_recent_signature(asset(1), RecentSignatureSource::Image, cx)
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.recent_signature_storage_issue.as_deref(),
            Some("This signature could not be saved to Recent.")
        )
    });
    fixture.keys.mode.store(0, Ordering::SeqCst);
    let entry = fixture
        .store
        .remember(asset(1), RecentSignatureSource::Image, 1)
        .unwrap()
        .signatures
        .remove(0);
    fixture.keys.mode.store(2, Ordering::SeqCst);
    view.update(cx, |view, cx| {
        view.remove_recent_signature(entry.id().into(), cx)
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.recent_signature_storage_issue.as_deref(),
            Some("Recent signatures could not be changed.")
        );
        assert!(!view.recent_signatures_loading);
    });
    fixture.keys.mode.store(0, Ordering::SeqCst);
    assert_eq!(fixture.store.list().unwrap().signatures.len(), 1);
}

#[gpui::test]
fn dismissal_rejects_late_load_and_clears_sensitive_state(cx: &mut TestAppContext) {
    let fixture = Fixture::new("dismiss");
    let view = fixture.workspace(cx);
    fixture
        .store
        .remember(asset(1), RecentSignatureSource::Typed, 1)
        .unwrap();
    view.update(cx, |view, cx| {
        view.load_recent_signatures(cx);
        view.signature_input_mode = SignatureInputMode::Type;
        view.drawn_signature
            .begin_stroke(crate::local_signature::NormalizedSignaturePoint::new(1, 1))
            .unwrap();
        view.signature_prepare_state = SignaturePrepareState::Preview(SignaturePreview {
            asset: asset(1),
            image: recent_signature_preview(fixture.store.list().unwrap().signatures.remove(0))
                .unwrap()
                .image,
        });
        view.dismiss_signature_popover(DocumentId::new(999), None, cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.recent_signatures.is_empty());
        assert!(!view.recent_signatures_loading);
        assert!(view.signature_name_input.is_none());
        assert!(view.drawn_signature.is_empty());
        assert!(matches!(
            view.signature_prepare_state,
            SignaturePrepareState::Idle
        ));
        assert!(matches!(
            view.signature_input_mode,
            SignatureInputMode::Draw
        ));
    });
}

#[gpui::test]
fn replacing_store_rejects_old_result(cx: &mut TestAppContext) {
    let old = Fixture::new("old");
    let new = Fixture::new("new");
    let view = old.workspace(cx);
    old.store
        .remember(asset(1), RecentSignatureSource::Typed, 1)
        .unwrap();
    view.update(cx, |view, cx| {
        view.load_recent_signatures(cx);
        view.bind_recent_signature_store(new.store.clone());
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.recent_signatures.is_empty()));
}

#[gpui::test]
fn clearing_platform_input_cancels_capture_and_discards_qr(cx: &mut TestAppContext) {
    let fixture = Fixture::new("platform-cancel");
    let view = fixture.workspace(cx);
    let operation = crate::phone_signature::SignatureOperation::default();
    let cancelled = operation.0.clone();
    view.update(cx, |view, cx| {
        view.signature_operation = Some(operation);
        view.clear_signature_input(cx);
        assert!(view.signature_operation.is_none());
        assert!(matches!(
            view.signature_prepare_state,
            SignaturePrepareState::Idle
        ));
    });
    assert!(cancelled.load(Ordering::Acquire));
}
