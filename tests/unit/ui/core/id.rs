use super::*;

#[test]
fn idpath_is_stable_for_same_path() {
    let a = IdPath::root("workbench")
        .push_str("sidebar")
        .push_str("tabs")
        .finish();
    let b = IdPath::root("workbench")
        .push_str("sidebar")
        .push_str("tabs")
        .finish();
    assert_eq!(a, b);
}

#[test]
fn idpath_differs_for_different_paths() {
    let a = IdPath::root("workbench").push_str("a").finish();
    let b = IdPath::root("workbench").push_str("b").finish();
    assert_ne!(a, b);

    let c = IdPath::root("workbench").push_u64(1).finish();
    let d = IdPath::root("workbench").push_u64(2).finish();
    assert_ne!(c, d);
}

#[test]
fn idpath_push_order_matters() {
    let a = IdPath::root("ns").push_str("a").push_str("bc").finish();
    let b = IdPath::root("ns").push_str("ab").push_str("c").finish();
    assert_ne!(a, b);
}

