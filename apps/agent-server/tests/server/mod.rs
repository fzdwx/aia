use super::ServerRunOptions;

#[test]
fn server_run_options_display_default_base_url() {
    let options = ServerRunOptions::default();
    assert_eq!(options.display_base_url(), aia_config::DEFAULT_SERVER_BASE_URL);
}

#[test]
fn server_run_options_display_localhost_for_wildcard_bind_addr() {
    let options = ServerRunOptions::default().with_bind_addr("0.0.0.0:4545");
    assert_eq!(options.display_base_url(), "http://localhost:4545");
}

#[test]
fn server_run_options_display_custom_host_for_specific_bind_addr() {
    let options = ServerRunOptions::default().with_bind_addr("127.0.0.1:4545");
    assert_eq!(options.display_base_url(), "http://127.0.0.1:4545");
}
