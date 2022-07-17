use testcontainers::{core::WaitFor, *};

const NAME: &str = "pablitx/ckb-testchain";
const TAG: &str = "latest";

#[derive(Debug, Default)]
pub struct CkbNode;

#[derive(Debug, Default, Clone)]
pub struct CkbNodeArgs;

impl ImageArgs for CkbNodeArgs {
    fn into_iterator(self) -> Box<dyn Iterator<Item = String>> {
        Box::new(
            vec![
                // "".to_string(),
            ]
            .into_iter(),
        )
    }
}

impl Image for CkbNode {
    type Args = CkbNodeArgs;

    fn name(&self) -> String {
        NAME.to_owned()
    }

    fn tag(&self) -> String {
        TAG.to_owned()
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![WaitFor::message_on_stdout(
            "Listen HTTP RPCServer on address",
        )]
    }
}

#[test]
fn ckb_image() {
    let _ = pretty_env_logger::try_init();
    let docker = clients::Cli::default();
    let node = docker.run(CkbNode);
    let host_port = node.get_host_port_ipv4(8114);

    let response = reqwest::blocking::Client::new()
        .post(&format!("http://127.0.0.1:{}", host_port))
        .body(
            json::object! {
                "jsonrpc" => "2.0",
                "method" => "get_tip_block_number",
                "params" => json::array![],
                "id" => 1
            }
            .dump(),
        )
        .header("content-type", "application/json")
        .send()
        .unwrap();

    let response = response.text().unwrap();
    let response = json::parse(&response).unwrap();

    assert_eq!(response["result"], "0x0");
}
