use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    about = "Create, configure, build, and deploy fullstack dapps running on Nervos Network",
    name = "trampoline",
    alias = "tram"
)]
pub enum TrampolineCommand {
    #[structopt(name = "new", alias = "n")]
    #[structopt(about = "Create a new Trampoline project")]
    NewProject { name: String },
    #[structopt(
        name = "network",
        alias = "net",
        about = "Manage local development chain"
    )]
    Network {
        #[structopt(flatten)]
        command: NetworkCommands,
    },
    #[structopt(
        name = "schema",
        about = "Manage custom on chain structures",
        alias = "s"
    )]
    Schema {
        #[structopt(flatten)]
        command: SchemaCommand,
    },
    // #[structopt(name = "account", about = "Manage addresses and keys")]
    // Account {
    //     #[structopt(flatten)]
    //     command: AccountCommand
    // }
}

#[derive(Debug, StructOpt)]
pub enum NetworkCommands {
    #[structopt(
        name = "init",
        about = "Init local development network using Docker compose",
        // alias = "lc"
    )]
    Init {
        #[structopt(short, long)]
        indexer: bool,
    },
    #[structopt(
        name = "start",
        about = "Launch local development network using Docker compose",
        alias = "s"
    )]
    Start {},
    #[structopt(
        name = "launch",
        about = "Launch local development network",
        alias = "l"
    )]
    Launch {},
    #[structopt(
        name = "set-miner",
        about = "Set the miner address so blocks can be mined locally"
    )]
    SetMiner {
        #[structopt(name = "pubkey", required_unless = "lock_arg", long)]
        pubkey: Option<String>,
        #[structopt(name = "lock_arg", required_unless = "pubkey", long)]
        lock_arg: Option<String>,
    },
    #[structopt(
        name = "config",
        alias = "c",
        about = "Configure your local developer network. You can also manually edit `trampoline-env.toml`"
    )]
    Config {
        #[structopt(name = "host-port", long, short)]
        port_host: Option<usize>,
        #[structopt(name = "host", long, short)]
        host: Option<String>,
        #[structopt(name = "local-path-binding", long, short)]
        local_binding: Option<PathBuf>,
    },
    #[structopt(name = "index", about = "Launch the indexer for improved queries")]
    Indexer {},
    #[structopt(
        name = "miner",
        about = "Start continuously mining blocks or mine a single block"
    )]
    Miner { one_block: Option<bool> },
    // #[structopt(
    //     name = "init",
    //     about = "Initialize new network configuration without starting"
    // )]
    // Init {},
    #[structopt(name = "rpc", about = "Make Rpc calls")]
    Rpc { hash: String },
}
#[derive(Debug, StructOpt)]
pub enum SchemaCommand {
    #[structopt(name = "new", about = "Initialize a new schema")]
    New { name: String, def: Option<String> },
    #[structopt(name = "build", about = "Generate rust bindings for schema")]
    Build { name: String },
}
