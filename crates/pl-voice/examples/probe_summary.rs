//! Summary of a VOICE-0 probe run (`pl_voice::probe`):
//! `cargo run -p pl-voice --example probe_summary -- <records.jsonl> [from_s] [to_s]`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: probe_summary <records.jsonl> [from_s] [to_s]");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("{path}: {error}");
        std::process::exit(1);
    });
    let seconds = |index: usize, default: f64| {
        args.get(index)
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(default)
    };
    let from = (seconds(1, 0.0) * 1000.0) as u64;
    let to = (seconds(2, f64::from(u32::MAX) / 1000.0) * 1000.0) as u64;
    let summary = pl_voice::probe::summarize(text.lines(), from, to);
    match serde_json::to_string_pretty(&summary) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("{error}"),
    }
}
