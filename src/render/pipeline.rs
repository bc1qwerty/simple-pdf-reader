use crate::backend::{PdfDocument, RenderedPage};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

pub struct RenderRequest {
    pub page_no: usize,
    pub scale: f32,
    pub rotation: i32,
    pub priority: RenderPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)]
pub enum RenderPriority {
    Prefetch = 0,
    Adjacent = 1,
    Current = 2,
}

pub struct RenderResult {
    pub page: RenderedPage,
}

enum WorkerCmd {
    Render(RenderRequest),
    Shutdown,
}

pub struct RenderPipeline {
    cmd_tx: Sender<WorkerCmd>,
    pub result_rx: Receiver<RenderResult>,
    worker: Option<JoinHandle<()>>,
    pending_count: usize,
}

impl RenderPipeline {
    pub fn new<D: PdfDocument + 'static>(doc: D, repaint_ctx: egui::Context) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCmd>();
        let (result_tx, result_rx) = mpsc::channel::<RenderResult>();

        let worker = thread::spawn(move || {
            Self::worker_loop(doc, cmd_rx, result_tx, repaint_ctx);
        });

        Self { cmd_tx, result_rx, worker: Some(worker), pending_count: 0 }
    }

    fn worker_loop<D: PdfDocument>(
        doc: D,
        rx: Receiver<WorkerCmd>,
        tx: Sender<RenderResult>,
        repaint_ctx: egui::Context,
    ) {
        while let Ok(cmd) = rx.recv() {
            match cmd {
                WorkerCmd::Render(first_req) => {
                    // Drain queue: keep only the latest request per page
                    let mut latest: HashMap<usize, RenderRequest> = HashMap::new();
                    latest.insert(first_req.page_no, first_req);

                    loop {
                        match rx.try_recv() {
                            Ok(WorkerCmd::Render(r)) => {
                                latest.insert(r.page_no, r);
                            }
                            Ok(WorkerCmd::Shutdown) => return,
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => return,
                        }
                    }

                    // Process by priority (Current first)
                    let mut requests: Vec<_> = latest.into_values().collect();
                    requests.sort_by(|a, b| b.priority.cmp(&a.priority));

                    for req in requests {
                        match doc.render_page_rotated(req.page_no, req.scale, req.rotation) {
                            Ok(page) => {
                                let _ = tx.send(RenderResult { page });
                                repaint_ctx.request_repaint();
                            }
                            Err(e) => {
                                eprintln!("[WARN] Render page {} failed: {e}", req.page_no);
                            }
                        }
                    }
                }
                WorkerCmd::Shutdown => break,
            }
        }
    }

    pub fn request(&mut self, page_no: usize, scale: f32, rotation: i32, priority: RenderPriority) {
        let _ = self.cmd_tx.send(WorkerCmd::Render(RenderRequest {
            page_no, scale, rotation, priority,
        }));
        self.pending_count += 1;
    }

    pub fn drain_results(&mut self) -> Vec<RenderResult> {
        let mut results = Vec::new();
        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    self.pending_count = self.pending_count.saturating_sub(1);
                    results.push(result);
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        results
    }

    pub fn has_pending(&self) -> bool {
        self.pending_count > 0
    }

    pub fn shutdown(&mut self) {
        let _ = self.cmd_tx.send(WorkerCmd::Shutdown);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RenderPipeline {
    fn drop(&mut self) {
        self.shutdown();
    }
}
