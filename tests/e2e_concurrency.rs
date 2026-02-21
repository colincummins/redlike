mod common;
use common::setup_test_server_and_test_client::setup_test_server_and_test_client;
use common::test_case::TestCase;
use redlike::server::server_from_listener;
const ADDR: &str = "127.0.0.1:0";
