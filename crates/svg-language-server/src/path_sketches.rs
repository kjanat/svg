//! Document-owned preview results and bounded, shared cold computations.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use svg_path_preview::Sketch;
use tokio::sync::{Semaphore, watch};

#[derive(Clone)]
enum State {
    Pending,
    Ready(Option<Arc<Sketch>>),
}

/// Each parsed document owns its cache. Workers may finish after close/edit,
/// but cannot retain results beyond the last reference to that document.
#[derive(Default)]
pub struct SketchCache(Mutex<HashMap<usize, watch::Receiver<State>>>);

#[derive(Clone)]
pub struct SketchScheduler {
    slots: Arc<Semaphore>,
}

impl Default for SketchScheduler {
    fn default() -> Self {
        Self {
            slots: Arc::new(Semaphore::new(2)),
        }
    }
}

impl SketchScheduler {
    /// Join an existing computation, or admit one without queueing CPU work.
    /// Saturation omits this request's preview without caching that omission.
    /// Cancelling a waiter leaves the shared, bounded worker available to others.
    pub async fn get_or_compute(
        &self,
        cache: &SketchCache,
        attribute_start: usize,
        build: impl FnOnce() -> Option<Sketch> + Send + 'static,
    ) -> Option<Arc<Sketch>> {
        let mut result = {
            let mut entries = cache.0.lock().ok()?;
            if let Some(result) = entries.get(&attribute_start) {
                let result = result.clone();
                drop(entries);
                result
            } else {
                let permit = Arc::clone(&self.slots).try_acquire_owned().ok()?;
                let (sender, result) = watch::channel(State::Pending);
                entries.insert(attribute_start, result.clone());
                drop(entries);
                tokio::task::spawn_blocking(move || {
                    let sketch = build().map(Arc::new);
                    drop(permit);
                    let _ = sender.send(State::Ready(sketch));
                });
                result
            }
        };
        loop {
            if let State::Ready(sketch) = &*result.borrow_and_update() {
                return sketch.clone();
            }
            result.changed().await.ok()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocumentState, SvgLanguageServer};
    use std::{
        future::Future,
        pin::pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::Poll,
        time::Duration,
    };
    use tokio::sync::oneshot;
    use tower_lsp_server::{
        LanguageServer, LspService,
        ls_types::{
            DidCloseTextDocumentParams, DocumentColorParams, HoverParams, PartialResultParams,
            Position, TextDocumentIdentifier, TextDocumentPositionParams, Uri,
            WorkDoneProgressParams,
        },
    };

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    fn document(
        source: &str,
    ) -> Result<Arc<DocumentState>, Box<dyn std::error::Error + Send + Sync>> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_svg::LANGUAGE.into())?;
        Ok(Arc::new(DocumentState {
            version: 1,
            source: source.to_owned(),
            tree: parser.parse(source, None).ok_or("parse")?,
            sketches: SketchCache::default(),
        }))
    }

    async fn pending(future: &mut (impl Future + Unpin)) {
        std::future::poll_fn(|cx| {
            assert!(std::pin::Pin::new(&mut *future).poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }

    #[tokio::test]
    async fn cancelled_waiter_does_not_duplicate_or_release_running_work() -> TestResult {
        let scheduler = SketchScheduler {
            slots: Arc::new(Semaphore::new(1)),
        };
        let cache = Arc::new(SketchCache::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let (started, running) = oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let first = tokio::spawn({
            let scheduler = scheduler.clone();
            let cache = Arc::clone(&cache);
            let calls = Arc::clone(&calls);
            async move {
                scheduler
                    .get_or_compute(&cache, 7, move || {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let _ = started.send(());
                        gate.recv().ok()?;
                        svg_path_preview::sketch("M0 0 L10 10")
                    })
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(5), running).await??;
        let mut second =
            pin!(scheduler.get_or_compute(&cache, 7, || panic!("duplicate cold computation")));
        pending(&mut second).await;
        first.abort();
        assert!(first.await.is_err());
        assert!(
            scheduler
                .get_or_compute(&cache, 8, || panic!("queued past capacity"))
                .await
                .is_none()
        );
        assert_eq!(scheduler.slots.available_permits(), 0);
        release.send(())?;
        let sketch = tokio::time::timeout(Duration::from_secs(5), second)
            .await?
            .ok_or("shared sketch")?;
        let cached = scheduler
            .get_or_compute(&cache, 7, || panic!("cache miss"))
            .await
            .ok_or("cached sketch")?;
        assert!(Arc::ptr_eq(&sketch, &cached));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(
            scheduler
                .get_or_compute(&cache, 8, || svg_path_preview::sketch("M0 0 L20 20"))
                .await
                .is_some()
        );
        assert!(scheduler.get_or_compute(&cache, 9, || None).await.is_none());
        assert!(
            scheduler
                .get_or_compute(&cache, 9, || panic!("negative result not cached"))
                .await
                .is_none()
        );
        Ok(())
    }

    #[tokio::test]
    async fn close_during_computation_reclaims_document_and_sketch() -> TestResult {
        let (service, _socket) = LspService::new(SvgLanguageServer::new);
        let server = service.inner();
        let uri: Uri = "file:///closed-preview.svg".parse()?;
        let doc = document("<svg><path d='M0 0 L10 10'/></svg>")?;
        let weak_doc = Arc::downgrade(&doc);
        let attribute_start = doc.source.find("d=").ok_or("attribute offset")?;
        server
            .documents
            .write()
            .await
            .insert(uri.clone(), Arc::clone(&doc));
        let (started, running) = oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let request = tokio::spawn({
            let scheduler = server.sketch_scheduler.clone();
            async move {
                let captured = Arc::clone(&doc);
                scheduler
                    .get_or_compute(&doc.sketches, attribute_start, move || {
                        let _ = started.send(());
                        gate.recv().ok()?;
                        assert_ne!(captured.source, "");
                        svg_path_preview::sketch("M0 0 L10 10")
                    })
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(5), running).await??;
        let mut hover = pin!(server.hover(HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: Position::new(0, u32::try_from(attribute_start)?),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        }));
        pending(&mut hover).await;
        // This runtime has one async thread. An unrelated LSP request and both
        // configuration writers must complete while the blocking worker pauses.
        tokio::time::timeout(Duration::from_secs(5), async {
            server
                .document_color(DocumentColorParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                    partial_result_params: PartialResultParams::default(),
                })
                .await?;
            drop(server.hover_settings.write().await);
            drop(server.runtime_compat.write().await);
            server
                .did_close(DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                })
                .await;
            TestResult::Ok(())
        })
        .await??;
        assert!(server.document_state(&uri).await.is_none());
        assert!(weak_doc.upgrade().is_some());
        release.send(())?;
        assert!(
            tokio::time::timeout(Duration::from_secs(5), hover)
                .await??
                .is_some()
        );
        let sketch = tokio::time::timeout(Duration::from_secs(5), request)
            .await??
            .ok_or("late sketch")?;
        assert!(weak_doc.upgrade().is_none(), "closed document retained");
        let weak_sketch = Arc::downgrade(&sketch);
        drop(sketch);
        // Publishing wakes waiters before the worker drops its sending channel.
        tokio::time::timeout(Duration::from_secs(5), async {
            while weak_sketch.upgrade().is_some() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn edit_during_computation_owns_a_fresh_cache() -> TestResult {
        let (service, _socket) = LspService::new(SvgLanguageServer::new);
        let server = service.inner();
        let uri: Uri = "file:///edited-preview.svg".parse()?;
        let old = document("<svg><path d='M0 0 L10 10'/></svg>")?;
        let weak_old = Arc::downgrade(&old);
        server
            .documents
            .write()
            .await
            .insert(uri.clone(), Arc::clone(&old));
        let (started, running) = oneshot::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let request = tokio::spawn({
            let scheduler = server.sketch_scheduler.clone();
            async move {
                let captured = Arc::clone(&old);
                scheduler
                    .get_or_compute(&old.sketches, 5, move || {
                        let _ = started.send(());
                        gate.recv().ok()?;
                        assert_ne!(captured.source, "");
                        svg_path_preview::sketch("M0 0 L10 10")
                    })
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(5), running).await??;
        server
            .update_document(
                uri.clone(),
                "<svg><path d='M0 0 L20 20'/></svg>".to_owned(),
                1,
            )
            .await;
        let new = server.document_state(&uri).await.ok_or("new document")?;
        let new_sketch = server
            .sketch_scheduler
            .get_or_compute(&new.sketches, 5, || svg_path_preview::sketch("M0 0 L20 20"))
            .await
            .ok_or("new sketch")?;
        release.send(())?;
        let old_sketch = tokio::time::timeout(Duration::from_secs(5), request)
            .await??
            .ok_or("old sketch")?;
        assert_eq!(old_sketch.width, 10.0);
        assert_eq!(new_sketch.width, 20.0);
        assert!(weak_old.upgrade().is_none());
        let cached = server
            .sketch_scheduler
            .get_or_compute(&new.sketches, 5, || panic!("new cache overwritten"))
            .await
            .ok_or("new cache")?;
        assert!(Arc::ptr_eq(&cached, &new_sketch));
        Ok(())
    }
}
