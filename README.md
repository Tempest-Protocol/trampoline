# Trampoline Framework
The framework for developing decentralized applications on Nervos Network's Common Knowledge Base.


# Motivation

The UTXO model is the primary alternative to the Account model. Nervos Network's state & programming model, termed the "cell"
model, is a generalization of the UTXO model to enhance programmability. Nervos Network is a UTXO-based smart contract platform.

There are myriad tradeoffs between UTXO and Account model - the choice depends on the use case and priorities of the developer.

One powerful feature of the UTXO model is that sophisticated composition of smart contract behavior can be realized in a single transaction, where the end result of the transaction is known a priori (before the transaction is submitted). This is because a transaction is a *complete* description of the proposed state change. This is in contrast to an Account model wherein the transaction, post-submission, can have unexpected results depending on what state changes occur prior to its execution while it's still sitting in the mempool.

Yet, this comes at a cost: *transaction generation* in a UTXO model is a lot more cumbersome. It takes a lot of custom code to *interact* with smart contracts because all of the custom smart contract logic requires a second reimplementation off-chain. The same goes for "composing" smart contract interactions. This is in contrast to the account model, wherein the other contracts with which a single smart contract interacts does not require additional implementation work for the off-chain client software.

The difficulty of building transaction generation logic affects testing as well: it is difficult to test the multitude of ways in which your smart contracts may interact (or not) with others, since building that interaction logic takes so much boilerplate.

Reducing the labor required to take advantage of composability on Nervos Network (the largest UTXO-based smart contract platform currently) is a primary goal of Trampoline.

To achieve this, Trampoline is in large part a transaction composition framework. The philosophy of Trampoline is that a smart contract is actually the *combination* of the on-chain scripts which *verify* conditions *and* the off-chain code which *generates* transactions to meet those conditions. Trampoline transaction generators take as inputs a pipeline of smart contracts and passes an empty transaction through this pipeline. Each smart contract then applies its respective updates to the entire transaction. This dramatically reduces the amount of code required when implementing a specific contract's logic. It also removes the burden of keeping track of all the various contract's logic when building composable transactions that include multiple smart contracts & their interactions.

Trampoline has other important goals as well, so here's a list of them:
1. Make composability far easier and quicker to achieve
2. Make transaction generation more declarative with less boilerplate
3. Enable easier end to end smart contract testing and simulation
4. Provide a single tool to streamline the entire dapp development experience
5. Support multi-network dapps and configurable network architectures


Goals 1 and 2 are achieved with Trampoline's pipeline-based approach to transaction generation. Goal 3 is also achieved by Trampoline's approach to composable transactions *as well as* Trampoline's simulation chain, which makes it possible to simulate end to end interactions over different time scales without even starting a network. Goal 4 is achieved with Trampoline's `create-react-app`-like experience to creating new dapp projects. Goal 5 is achieved by Trampoline's approach to orchestrating network services, which allows developers to spin up multiple nodes, miners, bridges to other chains, as well as layer 2 networks. This entire multi-chain, multi-layer environment can be configured by developers & launched locally in the time it takes to install Trampoline itself! Trampoline has simple defaults, but can easily scale the sophistication of the testing environment to fit the needs of the dapp developer, whether they're building a complex dapp on layer 1, a cross-chain dapp between Nervos Network and some other blockchain, or whether they're building on both layer 1 & layer 2.

## Example: Smart Contract Pipelines for SUDT (ERC20-equivalent)
Given the simultaneous importance & difficulty of composable transactions, below is an example of transaction generation with Trampoline's Generator API & Contract API. Also note that it uses the simulation chain (`MockChain`) and simulation chain rpc. The code would not be any different if used to send a tx to a local node.

The below is an example of *issuing* supply for a specific token.

```rust
        // sudt_contract is a specific trampoline::contract::Contract
        // Add an output rule, which will load in a Cell that uses the sudt contract & increases its balance by 2000
        sudt_contract.add_output_rule(
            ContractCellFieldSelector::Data,
            |amount: ContractCellField<Byte32, Uint128>| -> ContractCellField<Byte32, Uint128> {
                if let ContractCellField::Data(amount) = amount {
                    let mut amt_bytes = [0u8; 16];
                    amt_bytes.copy_from_slice(amount.as_slice());
                    let amt = u128::from_le_bytes(amt_bytes) + 2000;
                    ContractCellField::Data(amt.pack())
                } else {
                    amount
                }
            },
        );

        // Add an input rule. This will be used by the Generator to gather the correct input Cells
        sudt_contract.add_input_rule(move |_tx| -> CellQuery {
            CellQuery {
                _query: QueryStatement::Single(CellQueryAttribute::LockHash(
                    minter_lock_hash.clone().into(),
                )),
                _limit: 1,
            }
        });

        // Instantiate simulation chain rpc and tx generator
        // Add the sudt contract to the generator pipeline
        let chain_rpc = ChainRpc::new(MockChain::default());
        let generator = Generator::new()
            .chain_service(&chain_rpc)
            .query_service(&chain_rpc)
            .pipeline(vec![&sudt_contract]);

        // Generate the transaction structure
        let tx = generator.generate();
        // Verify the transaction (simulate it to ensure the transaction succeeds)
        let is_valid = chain_rpc.verify_tx(&tx);

        // If it is valid, send the tx (thereby persisting udpates to the simulation chain)
        if is_valid {
            chain_rpc.send_tx(tx);
        }


```

# Features
Trampoline allows you to jump straight into dapp development without worrying about pesky configuration, installing
a bunch of different tools, etc.

Trampoline projects include all components of a dapp so that you can manage all of the moving parts in one place in
coherent development environment.

- [x] Quickly generate new trampoline projects.
- [x] Start and stop local dev chain with ease.
- [x] Start and stop local indexer(s) with ease.
- [x] Add your own miner(s).
- [x] Autogenerate Rust bindings for custom schemas for use on and off chain.
- [x] Contract API for partial transaction creation based on custom logic
- [x] Generator API for transaction generation with Contract pipelines
- [x] Simulation chain for testing without running external services (WIP)
- [ ]  Manage accounts and addresses across developer, staging, and deployment environments.
- [ ]  Indexer extensions to index custom schemas.
- [ ]  Trampoline server API powered by Rocket-rs for transaction generation & querying.
- [ ]  Declaratively define transaction patterns for easy transaction creation.
- [ ]  Compile transaction patterns to CKB scripts 


# Installation
Currently, just ensure the Rust toolchain is installed on your machine. Then, install with Cargo:

`cargo install trampoline --git https://github.com/WilfredTA/trampoline`

You also need to have Docker installed.

# Usage

## Start a new project
`trampoline new <project_name>`

## Manage local network

To initialize and start a new network: `trampoline network launch`

To set a miner: `trampoline network set-miner [lock_arg | pubkey]`

To start the miner: `trampoline network miner`

To start an indexer: `trampoline network indexer`

## Manage schemas

Create a new schema: `trampoline schema new <schema_name>`

Optionally, schema definition (in Molecule) can be passed in: `trampoline schema new byte_10_arr "array my_array [byte; 10]"`

Generate rust bindings to build and decode schema: `trampoline schema build <schema_name>`


# Trampoline Projects
Trampoline projects provide a structure to manage on- and off-chain code. A Trampoline project makes code reuse and collaboration easy by automatically exporting libraries for use by other contracts and other applications.
- scripts
- schemas
- tests
- generators

