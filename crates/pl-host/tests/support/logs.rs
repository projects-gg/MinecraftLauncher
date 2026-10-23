//! Captures the test process's tracing events the way the launcher logs them: `pl_host` at every
//! level, other crates from INFO up (`info,pl_=debug`, and more). Records of the `log` crate are not
//! bridged here.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INSTALLED: OnceLock<()> = OnceLock::new();

/// Installs the capture as the process's subscriber (once).
pub fn install() {
    INSTALLED.get_or_init(|| {
        tracing::subscriber::set_global_default(Capture {
            next_span: AtomicU64::new(1),
        })
        .expect("no other subscriber in this test binary");
    });
}

/// Every line captured so far.
pub fn lines() -> Vec<String> {
    LINES.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn push(line: String) {
    LINES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(line);
}

struct Capture {
    next_span: AtomicU64,
}

struct Fields<'a>(&'a mut String);

impl Visit for Fields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, " {}={value:?}", field.name());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let _ = write!(self.0, " {}={value}", field.name());
    }
}

impl Subscriber for Capture {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target().starts_with("pl_host") || *metadata.level() <= Level::INFO
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        let mut line = format!(
            "span {} {}",
            span.metadata().target(),
            span.metadata().name()
        );
        span.record(&mut Fields(&mut line));
        push(line);
        Id::from_u64(self.next_span.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _span: &Id, values: &Record<'_>) {
        let mut line = String::from("record");
        values.record(&mut Fields(&mut line));
        push(line);
    }

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let metadata = event.metadata();
        let mut line = format!("{} {}", metadata.level(), metadata.target());
        event.record(&mut Fields(&mut line));
        push(line);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}
