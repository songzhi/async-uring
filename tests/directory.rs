#[test]
fn basic_remove_dir() {
    async_uring::start(async {
        let temp_dir = tempfile::TempDir::new().unwrap();
        async_uring::fs::remove_dir(temp_dir.path()).await.unwrap();
        assert!(std::fs::metadata(temp_dir.path()).is_err());
    });
}
