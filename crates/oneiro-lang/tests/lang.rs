use oneiro_lang::{parse, StmtKind};

#[test]
fn colors_and_comments_are_distinguished() {
    let (f, d) = parse("# a comment\ncharacter A { head: x { skin_tone: #F2C9A0 } } # trailing\n", "t.onr");
    assert!(d.is_empty(), "{:?}", d);
    assert_eq!(f.blocks.len(), 1);
}

#[test]
fn parses_scene_statements() {
    let src = r#"oneiro 0.1
use character A
use set s
scene 1 "T" in s {
  place A at s.x facing north
  camera medium on A from left move dolly_in for 2s
  cut to over_shoulder A -> B
  A (happy): "hi" audio "a.wav"
  A walks_to s.y and waves
  together {
    A nods
    B shrugs
  }
  wait 500ms
}
"#;
    let (f, d) = parse(src, "t.onr");
    assert!(d.is_empty(), "{:?}", d);
    let stmts = &f.scenes[0].stmts;
    assert!(matches!(stmts[0].kind, StmtKind::Place(_)));
    assert!(matches!(stmts[1].kind, StmtKind::Camera(_)));
    assert!(matches!(stmts[3].kind, StmtKind::Dialogue(_)));
    // `and` produces a parallel group
    assert!(matches!(stmts[4].kind, StmtKind::Together(ref v) if v.len() == 2));
    match &stmts[6].kind {
        StmtKind::Wait(w) => assert!((w - 0.5).abs() < 1e-9),
        other => panic!("{:?}", other),
    }
}

#[test]
fn reports_errors_with_recovery() {
    let src = "scene 1 \"T\" in s {\n  A (happy) \"missing colon\"\n  B walks_to\n  C (sad): \"ok\"\n}\n";
    let (f, d) = parse(src, "t.onr");
    assert!(!d.is_empty(), "{:?}", d);
    assert!(d.iter().all(|x| x.code.starts_with('E')));
    // The parser recovered and still parsed the later lines.
    assert!(f.scenes[0].stmts.iter().any(|s| matches!(s.kind, StmtKind::Dialogue(_))));
}

#[test]
fn missing_brace_is_reported() {
    let (_, d) = parse("scene 1 \"T\" in s {\n  wait 1s\n", "t.onr");
    assert!(d.iter().any(|x| x.code == "E0106"), "{:?}", d);
}
