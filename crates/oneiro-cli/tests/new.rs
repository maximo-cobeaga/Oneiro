use std::process::Command;

#[test]
fn new_scaffolds_a_project_that_checks_and_refuses_to_overwrite() {
    let dir = std::env::temp_dir().join(format!("oneiro_new_{}", std::process::id())).join("my-show");
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    let exe = env!("CARGO_BIN_EXE_oneiro");
    let run = |args: &[&str]| Command::new(exe).args(args).output().unwrap();
    let out = run(&["new", dir.to_str().unwrap(), "--name", "My Show"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(std::fs::read_to_string(dir.join("oneiro.toml")).unwrap().contains("My Show"));
    let chk = run(&["check", dir.to_str().unwrap()]);
    assert!(chk.status.success(), "{}{}", String::from_utf8_lossy(&chk.stdout), String::from_utf8_lossy(&chk.stderr));
    let again = run(&["new", dir.to_str().unwrap()]);
    assert!(!again.status.success());
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}
