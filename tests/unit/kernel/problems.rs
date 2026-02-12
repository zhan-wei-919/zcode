use super::*;
use std::path::Path;
use std::time::Instant;

fn mk_problem(path: &Path, line: u32, col: u32, message: &str) -> ProblemItem {
    ProblemItem {
        path: path.to_path_buf(),
        range: ProblemRange {
            start_line: line,
            start_col: col,
            end_line: line,
            end_col: col.saturating_add(1),
        },
        severity: ProblemSeverity::Error,
        message: message.to_string(),
        source: Some("baseline-test".to_string()),
    }
}

#[test]
fn test_update_path_sorts_by_path_then_position() {
    let mut state = ProblemsState::default();
    let path_a = PathBuf::from("src/a.rs");
    let path_b = PathBuf::from("src/b.rs");

    assert!(state.update_path(
        path_b.clone(),
        vec![
            mk_problem(&path_b, 12, 5, "b-12"),
            mk_problem(&path_b, 2, 9, "b-2"),
        ],
    ));
    assert!(state.update_path(
        path_a.clone(),
        vec![
            mk_problem(&path_a, 8, 1, "a-8"),
            mk_problem(&path_a, 3, 4, "a-3"),
        ],
    ));

    let ordered: Vec<(PathBuf, u32, u32)> = state
        .items()
        .iter()
        .map(|item| {
            (
                item.path.clone(),
                item.range.start_line,
                item.range.start_col,
            )
        })
        .collect();
    assert_eq!(
        ordered,
        vec![
            (path_a.clone(), 3, 4),
            (path_a, 8, 1),
            (path_b.clone(), 2, 9),
            (path_b, 12, 5),
        ]
    );
}

#[test]
fn test_update_path_with_same_items_reports_unchanged() {
    let mut state = ProblemsState::default();
    let path = PathBuf::from("src/same.rs");
    let items = vec![
        mk_problem(&path, 1, 2, "same-1"),
        mk_problem(&path, 4, 3, "same-2"),
    ];

    assert!(state.update_path(path.clone(), items.clone()));
    assert!(!state.update_path(path, items));
    assert_eq!(state.items().len(), 2);
}

#[test]
fn test_update_path_empty_items_removes_entries() {
    let mut state = ProblemsState::default();
    let path = PathBuf::from("src/remove.rs");
    assert!(state.update_path(path.clone(), vec![mk_problem(&path, 10, 0, "to-remove")]));
    assert_eq!(state.items().len(), 1);

    assert!(state.update_path(path, Vec::new()));
    assert!(state.items().is_empty());
}

#[test]
fn test_update_path_inserts_between_existing_paths() {
    let mut state = ProblemsState::default();
    let path_a = PathBuf::from("src/a.rs");
    let path_b = PathBuf::from("src/b.rs");
    let path_c = PathBuf::from("src/c.rs");

    assert!(state.update_path(
        path_a.clone(),
        vec![
            mk_problem(&path_a, 1, 0, "a-1"),
            mk_problem(&path_a, 4, 0, "a-4")
        ],
    ));
    assert!(state.update_path(
        path_c.clone(),
        vec![
            mk_problem(&path_c, 2, 0, "c-2"),
            mk_problem(&path_c, 8, 0, "c-8")
        ],
    ));
    assert!(state.update_path(
        path_b.clone(),
        vec![
            mk_problem(&path_b, 3, 1, "b-3"),
            mk_problem(&path_b, 9, 1, "b-9")
        ],
    ));

    let ordered_paths: Vec<PathBuf> = state.items().iter().map(|item| item.path.clone()).collect();
    assert_eq!(
        ordered_paths,
        vec![
            path_a.clone(),
            path_a,
            path_b.clone(),
            path_b,
            path_c.clone(),
            path_c,
        ]
    );
}

#[test]
fn test_update_path_replace_length_shifts_following_ranges() {
    let mut state = ProblemsState::default();
    let path_a = PathBuf::from("src/a.rs");
    let path_b = PathBuf::from("src/b.rs");
    let path_c = PathBuf::from("src/c.rs");

    let items_a = vec![
        mk_problem(&path_a, 1, 0, "a-1"),
        mk_problem(&path_a, 2, 0, "a-2"),
    ];
    let items_b_short = vec![mk_problem(&path_b, 1, 0, "b-1")];
    let items_b_long = vec![
        mk_problem(&path_b, 1, 0, "b-1"),
        mk_problem(&path_b, 2, 0, "b-2"),
        mk_problem(&path_b, 3, 0, "b-3"),
        mk_problem(&path_b, 4, 0, "b-4"),
    ];
    let items_c = vec![
        mk_problem(&path_c, 1, 0, "c-1"),
        mk_problem(&path_c, 2, 0, "c-2"),
    ];

    assert!(state.update_path(path_a.clone(), items_a));
    assert!(state.update_path(path_b.clone(), items_b_short));
    assert!(state.update_path(path_c.clone(), items_c.clone()));

    assert!(state.update_path(path_b.clone(), items_b_long));
    assert!(!state.update_path(path_c.clone(), items_c.clone()));
    assert!(state.update_path(
        path_c,
        vec![mk_problem(&PathBuf::from("src/c.rs"), 9, 0, "c-9")]
    ));
}

#[test]
fn test_update_path_remove_middle_shifts_following_ranges() {
    let mut state = ProblemsState::default();
    let path_a = PathBuf::from("src/a.rs");
    let path_b = PathBuf::from("src/b.rs");
    let path_c = PathBuf::from("src/c.rs");

    assert!(state.update_path(
        path_a.clone(),
        vec![
            mk_problem(&path_a, 1, 0, "a-1"),
            mk_problem(&path_a, 2, 0, "a-2")
        ],
    ));
    assert!(state.update_path(
        path_b.clone(),
        vec![
            mk_problem(&path_b, 1, 0, "b-1"),
            mk_problem(&path_b, 2, 0, "b-2"),
            mk_problem(&path_b, 3, 0, "b-3"),
        ],
    ));
    let items_c = vec![
        mk_problem(&path_c, 4, 0, "c-4"),
        mk_problem(&path_c, 5, 0, "c-5"),
    ];
    assert!(state.update_path(path_c.clone(), items_c.clone()));

    assert!(state.update_path(path_b, Vec::new()));
    assert_eq!(state.items().len(), 4);
    assert!(!state.update_path(path_c.clone(), items_c));
    assert!(state.update_path(
        path_c,
        vec![mk_problem(&PathBuf::from("src/c.rs"), 7, 0, "c-7")]
    ));
}

#[test]
fn experiment_problems_update_path_scale_baseline() {
    let mut state = ProblemsState::default();
    let files = 900usize;
    let per_file = 8usize;
    let loops = 220usize;

    for file in 0..files {
        let path = PathBuf::from(format!("src/file_{file:04}.rs"));
        let items: Vec<ProblemItem> = (0..per_file)
            .map(|i| mk_problem(&path, (i * 3) as u32, (i % 7) as u32, "seed"))
            .collect();
        assert!(state.update_path(path, items));
    }
    let baseline_total = files * per_file;
    assert_eq!(state.items().len(), baseline_total);

    let hot_path = PathBuf::from(format!("src/file_{:04}.rs", files / 2));
    let mut changed_count = 0usize;
    let start = Instant::now();
    for round in 0..loops {
        let shift = (round % 2) as u32;
        let items: Vec<ProblemItem> = (0..per_file)
            .map(|i| {
                mk_problem(
                    &hot_path,
                    (i * 3) as u32 + shift,
                    (i % 7) as u32,
                    if shift == 0 { "hot-even" } else { "hot-odd" },
                )
            })
            .collect();
        changed_count += usize::from(state.update_path(hot_path.clone(), items));
    }
    let elapsed = start.elapsed();
    let avg_us = elapsed.as_secs_f64() * 1_000_000.0 / loops as f64;

    eprintln!(
        "[experiment] problems_update_path loops={} files={} per_file={} total_items={} elapsed_ms={} avg_us={:.2} changed_count={}",
        loops,
        files,
        per_file,
        state.items().len(),
        elapsed.as_millis(),
        avg_us,
        changed_count
    );

    assert_eq!(state.items().len(), baseline_total);
    assert_eq!(changed_count, loops);
}
