use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("btc-wallet-test-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_btc-wallet-bench"))
            .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .args(args)
            .output()
            .unwrap()
    }
    fn build(&self) -> PathBuf {
        let dataset = self.0.join("data");
        let result = self.run(&[
            "build",
            "--catalog",
            "evals/wallet-policy-v1.json",
            "--out",
            dataset.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        dataset
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn references_pass_and_missing_answers_remain_in_denominator() {
    let work = Workspace::new();
    let dataset = work.build();
    let bytes = fs::read(dataset.join("fixtures.jsonl")).unwrap();
    let again = work.run(&[
        "build",
        "--catalog",
        "evals/wallet-policy-v1.json",
        "--out",
        dataset.to_str().unwrap(),
    ]);
    assert!(!again.status.success());
    assert_eq!(bytes, fs::read(dataset.join("fixtures.jsonl")).unwrap());
    let empty = work.0.join("empty.jsonl");
    fs::write(&empty, "").unwrap();
    for (name, responses, expected) in [
        ("references", dataset.join("references.jsonl"), 20),
        ("empty", empty, 0),
    ] {
        let out = work.0.join(name);
        let result = work.run(&[
            "grade",
            "--dataset",
            dataset.to_str().unwrap(),
            "--responses",
            responses.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let summary: Value =
            serde_json::from_slice(&fs::read(out.join("summary.json")).unwrap()).unwrap();
        for kind in ["template", "concrete"] {
            assert_eq!(summary[kind]["total"], 20);
            assert_eq!(summary[kind]["correct"], expected);
        }
    }
}

#[test]
fn duplicate_or_unknown_response_ids_are_rejected() {
    let work = Workspace::new();
    let dataset = work.build();
    let references = fs::read_to_string(dataset.join("references.jsonl")).unwrap();
    let first = references.lines().next().unwrap();
    for (name, text) in [
        ("duplicate", format!("{first}\n{first}\n")),
        (
            "unknown",
            "{\"task_id\":\"absent\",\"answer\":\"tr(@0/**)\"}\n".into(),
        ),
    ] {
        let responses = work.0.join(format!("{name}.jsonl"));
        fs::write(&responses, text).unwrap();
        let result = work.run(&[
            "grade",
            "--dataset",
            dataset.to_str().unwrap(),
            "--responses",
            responses.to_str().unwrap(),
            "--out",
            work.0.join(name).to_str().unwrap(),
        ]);
        assert!(!result.status.success());
    }
}

#[test]
fn training_export_rejects_evaluation_and_keeps_splits_separate() {
    let work = Workspace::new();
    let out = work.0.join("training");
    let denied = work.run(&[
        "build-training",
        "--catalog",
        "evals/wallet-policy-v1.json",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(!denied.status.success());
    assert!(!out.exists());
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals/wallet-policy-v1.json");
    let mut catalog: Value = serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
    catalog["evaluation_only"] = false.into();
    catalog["cases"].as_array_mut().unwrap().truncate(1);
    let path = work.0.join("catalog.json");
    fs::write(&path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    let built = work.run(&[
        "build-training",
        "--catalog",
        path.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let traces: Value =
        serde_json::from_slice(&fs::read(out.join("traces.json")).unwrap()).unwrap();
    assert_eq!(traces.as_array().unwrap().len(), 2);
    let denied = work.run(&["audit", "--dataset", out.to_str().unwrap()]);
    assert!(!denied.status.success());
    let denied = work.run(&[
        "build-evaluation",
        "--catalog",
        path.to_str().unwrap(),
        "--out",
        work.0.join("eval").to_str().unwrap(),
    ]);
    assert!(!denied.status.success());
}
