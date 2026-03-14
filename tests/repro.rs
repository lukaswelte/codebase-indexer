use std::path::PathBuf;

#[test]
fn test_strip_prefix_fixed() {
    // Current setup in main.rs: root is "." canonicalized
    let root = PathBuf::from(".").canonicalize().unwrap();
    
    // Path from notify: absolute
    let absolute_path = std::env::current_dir().unwrap().join("src/main.rs");
    
    // This should now work as both are absolute
    let rel_path = absolute_path.strip_prefix(&root);
    assert!(rel_path.is_ok(), "strip_prefix failed with canonicalized root: {:?}", rel_path.err());
    assert_eq!(rel_path.unwrap(), PathBuf::from("src/main.rs"));
}
