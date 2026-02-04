use super::*;

#[test]
fn base64_encode_matches_known_vectors() {
    assert_eq!(
        build_sequence("", Osc52Env { is_tmux: false }).unwrap(),
        "\x1b]52;c;\x07"
    );

    let seq = build_sequence("hello", Osc52Env { is_tmux: false }).unwrap();
    assert!(seq.starts_with(OSC52_PREFIX));
    assert!(seq.ends_with(OSC52_SUFFIX_BEL));
    assert_eq!(seq, "\x1b]52;c;aGVsbG8=\x07");
}

#[test]
fn osc52_sequence_wraps_for_tmux() {
    let seq = build_sequence("hi", Osc52Env { is_tmux: true }).unwrap();
    assert_eq!(seq, "\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\");
}

#[test]
fn osc52_rejects_large_payloads() {
    let big = "x".repeat(OSC52_MAX_BYTES + 1);
    let err = build_sequence(&big, Osc52Env::default()).unwrap_err();
    assert_eq!(
        err,
        Osc52Error::TooLarge {
            bytes: OSC52_MAX_BYTES + 1
        }
    );
}
