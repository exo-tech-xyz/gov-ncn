#[cfg(test)]
mod tests {
    use crate::fixtures::ncn_program_client::assert_ncn_program_error;
    use crate::fixtures::{test_builder::TestBuilder, TestResult};
    use jito_restaking_core::{config::Config, ncn_vault_ticket::NcnVaultTicket};
    use ncn_program_client::types::MetaMerkleSnapshot;
    use ncn_program_core::error::NCNProgramError;
    use solana_sdk::{hash::hash, signature::Keypair, signer::Signer};
    use std::fs::File;
    use std::io::BufReader;

    fn read_meta_merkle_snapshot(path: &str) -> MetaMerkleSnapshot {
        let file = File::open(path).expect("Failed to open file");
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).expect("Failed to deserialize")
    }

    #[tokio::test]
    async fn verify_merkle() -> TestResult<()> {
        let path = format!(
            "{}/tests/fixtures/meta_merkle.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let meta_merkle_snapshot = read_meta_merkle_snapshot(&path);

        // 0. Building the test environment
        let mut fixture = TestBuilder::new().await;
        fixture.initialize_restaking_and_vault_programs().await?;

        let mut ncn_program_client = fixture.ncn_program_client();
        let mut vault_program_client = fixture.vault_client();
        let mut restaking_client = fixture.restaking_program_client();

        // 1. Preparing the test variables
        const OPERATOR_COUNT: usize = 3; // Number of operators to create for testing
        let mint = Keypair::new(); // govSOL Mint.
        let delegation_amount: u64 = 100_000_000_000; // Default uniform delegation.
        let operator_fee_bps = 0; // No fees involved.

        // 2. Initializing all the needed accounts using Jito's Staking and Vault programs
        // this step will initialize the NCN account, and all the operators and vaults accounts,
        // it will also initialize the handshake relationships between all the NCN components

        // 2.a. Initialize the NCN account using the Jito Restaking program
        let mut test_ncn = fixture.create_test_ncn().await?;
        let ncn_pubkey = test_ncn.ncn_root.ncn_pubkey;

        // 2.b. Initialize the operators using the Jito Restaking program, and initiate the
        //   handshake relationship between the NCN <> operators
        // Creates OPERATOR_COUNT operators and associates them with the NCN.
        fixture
            .add_operators_to_test_ncn(&mut test_ncn, OPERATOR_COUNT, Some(operator_fee_bps))
            .await?;

        // 2.c. Initialize the vaults using the Vault program By Jito
        // and initiate the handshake relationship between the NCN <> vaults, and vaults <> operators
        fixture
            .add_vaults_to_test_ncn(&mut test_ncn, 1, Some(mint.insecure_clone()))
            .await?;

        // 2.d. Vaults delegate stakes to operators
        // Each vault delegates different amounts to different operators based on the delegation amounts array
        for operator_root in &test_ncn.operators {
            vault_program_client
                .do_add_delegation(
                    &test_ncn.vaults[0],
                    &operator_root.operator_pubkey,
                    delegation_amount,
                )
                .await
                .unwrap();
        }

        // 2.e Fast-forward time to simulate a full epoch passing
        // This is needed for all the relationships to finish warming up
        let restaking_config_address =
            Config::find_program_address(&jito_restaking_program::id()).0;
        let restaking_config = restaking_client
            .get_config(&restaking_config_address)
            .await?;
        let epoch_length = restaking_config.epoch_length();
        fixture
            .warp_slot_incremental(epoch_length * 2)
            .await
            .unwrap();

        // 3. Setting up the NCN-program
        // every thing here will be a call for an instruction to the NCN program that the NCN admin
        // is suppose to deploy to the network.
        {
            // 3.a. Initialize the config for the ncn-program
            ncn_program_client
                .do_initialize_config(test_ncn.ncn_root.ncn_pubkey, &test_ncn.ncn_root.ncn_admin)
                .await?;

            // 3.b Initialize the vault_registry - creates accounts to track vaults
            ncn_program_client
                .do_full_initialize_vault_registry(test_ncn.ncn_root.ncn_pubkey)
                .await?;

            // 3.c. Register all the ST (Support Token) mints in the ncn program
            // This assigns weights to each mint for voting power calculations
            ncn_program_client
                .do_admin_register_st_mint(ncn_pubkey, mint.pubkey(), 1)
                .await?;

            // 4.d Register all the vaults in the ncn program
            // note that this is permissionless because the admin already approved it by initiating
            // the handshake before
            for vault in test_ncn.vaults.iter() {
                let vault = vault.vault_pubkey;
                let (ncn_vault_ticket, _, _) = NcnVaultTicket::find_program_address(
                    &jito_restaking_program::id(),
                    &ncn_pubkey,
                    &vault,
                );

                ncn_program_client
                    .do_register_vault(ncn_pubkey, vault, ncn_vault_ticket)
                    .await?;
            }
        }

        // At this point, all the preparations and configurations are done, everything else after
        // this is part of the consensus cycle, so it depends on the way you setup your voting system
        // you will have to run the code below
        //
        // in this example, the voting is cyclical, and per epoch, so the code you will see below
        // will run per epoch to prepare for the voting

        // 4. Prepare the voting environment
        {
            // 4.a. Initialize the epoch state - creates a new state for the current epoch
            fixture.add_epoch_state_for_test_ncn(&test_ncn).await?;
            // 4.b. Initialize the weight table - prepares the table that will track voting weights
            let clock = fixture.clock().await;
            let epoch = clock.epoch;
            ncn_program_client
                .do_full_initialize_weight_table(test_ncn.ncn_root.ncn_pubkey, epoch)
                .await?;

            // 4.c. Take a snapshot of the weights for each ST mint
            // This records the current weights for the voting calculations
            ncn_program_client
                .do_set_epoch_weights(test_ncn.ncn_root.ncn_pubkey, epoch)
                .await?;
            // 4.d. Take the epoch snapshot - records the current state for this epoch
            fixture.add_epoch_snapshot_to_test_ncn(&test_ncn).await?;
            // 4.e. Take a snapshot for each operator - records their current stakes
            fixture
                .add_operator_snapshots_to_test_ncn(&test_ncn)
                .await?;
            // 4.f. Take a snapshot for each vault and its delegation - records delegations
            fixture
                .add_vault_operator_delegation_snapshots_to_test_ncn(&test_ncn)
                .await?;

            // 4.g. Initialize the ballot box - creates the voting container for this epoch
            fixture.add_ballot_box_to_test_ncn(&test_ncn).await?;
        }

        let winning_merkle = meta_merkle_snapshot.root;
        let winning_snapshot = hash(b"snapshot1").to_bytes();

        // 5. Cast votes from operators
        let epoch = fixture.clock().await.epoch;
        {
            for operator_root in test_ncn.operators.iter() {
                let operator = operator_root.operator_pubkey;

                ncn_program_client
                    .do_cast_vote(
                        ncn_pubkey,
                        operator,
                        &operator_root.operator_admin,
                        winning_merkle,
                        winning_snapshot,
                        epoch,
                    )
                    .await?;
            }
        }

        // 6. Verify for some vote accounts
        for i in 0..10 {
            let merkle_leaf_bundle = meta_merkle_snapshot.leaf_bundles[i * 10].clone();

            let meta_merkle_proof = merkle_leaf_bundle.proof.unwrap();
            let meta_merkle_leaf = merkle_leaf_bundle.meta_merkle_leaf;
            ncn_program_client
                .do_verify_merkle(
                    ncn_pubkey,
                    epoch,
                    meta_merkle_proof.clone(),
                    meta_merkle_leaf.clone(),
                    None,
                    None,
                )
                .await?;
        }

        // 7. Expect failure for wrong proofs.
        for i in 0..10 {
            let merkle_leaf_bundle1 = meta_merkle_snapshot.leaf_bundles[i + 5].clone();
            let merkle_leaf_bundle2 = meta_merkle_snapshot.leaf_bundles[i + 3].clone();

            let meta_merkle_proof = merkle_leaf_bundle1.proof.unwrap();
            let meta_merkle_leaf = merkle_leaf_bundle2.meta_merkle_leaf;
            let result = ncn_program_client
                .do_verify_merkle(
                    ncn_pubkey,
                    epoch,
                    meta_merkle_proof.clone(),
                    meta_merkle_leaf.clone(),
                    None,
                    None,
                )
                .await;
            assert_ncn_program_error(result, NCNProgramError::InvalidProof, Some(0));
        }

        // TODO: Test proof max size
        // TODO: Test invalid consensus result

        Ok(())
    }
}
