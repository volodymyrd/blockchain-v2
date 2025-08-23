# blockchain-v2

cargo run --package blockchain-keygen -- new --no-passphrase -fso config/faucet.json
cargo run --package blockchain-keygen -- new --no-passphrase -so config/leader/identity.json
cargo run --package blockchain-keygen -- new --no-passphrase -so config/leader/stake-account.json
cargo run --package blockchain-keygen -- new --no-passphrase -so config/leader/vote-account.json
