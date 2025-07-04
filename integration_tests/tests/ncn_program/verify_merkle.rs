#[cfg(test)]
mod tests {
    use crate::fixtures::assert_ix_error;
    use crate::fixtures::ncn_program_client::{assert_ncn_program_error, NCNProgramClient};
    use crate::fixtures::test_builder::TestNcn;
    use crate::fixtures::{test_builder::TestBuilder, TestResult};
    use jito_restaking_core::{config::Config, ncn_vault_ticket::NcnVaultTicket};
    use meta_merkle_tree::merkle_tree::MerkleTree;
    use ncn_program_client::types::{MetaMerkleSnapshot, StakeMerkleLeaf};
    use ncn_program_core::error::NCNProgramError;
    use solana_sdk::instruction::InstructionError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{
        hash::{hash, hashv, Hash},
        signature::Keypair,
        signer::Signer,
    };
    use std::fs::File;
    use std::io::BufReader;

    fn read_meta_merkle_snapshot(path: &str) -> MetaMerkleSnapshot {
        let file = File::open(path).expect("Failed to open file");
        let reader = BufReader::new(file);
        serde_json::from_reader(reader).expect("Failed to deserialize")
    }

    fn get_stake_merkle_proof(
        stake_merkle_leaves: Vec<StakeMerkleLeaf>,
        index: usize,
    ) -> Vec<[u8; 32]> {
        pub fn hash(leaf: &StakeMerkleLeaf) -> Hash {
            hashv(&[
                &leaf.voting_wallet.to_bytes(),
                &leaf.stake_account.to_bytes(),
                &leaf.active_stake.to_le_bytes(),
            ])
        }

        fn get_proof(merkle_tree: &MerkleTree, index: usize) -> Vec<[u8; 32]> {
            let mut proof = Vec::new();
            let path = merkle_tree.find_path(index).expect("path to index");
            for branch in path.get_proof_entries() {
                if let Some(hash) = branch.get_left_sibling() {
                    proof.push(hash.to_bytes());
                } else if let Some(hash) = branch.get_right_sibling() {
                    proof.push(hash.to_bytes());
                } else {
                    panic!("expected some hash at each level of the tree");
                }
            }
            proof
        }

        let hashed_nodes: Vec<[u8; 32]> = stake_merkle_leaves
            .iter()
            .map(|n| hash(n).to_bytes())
            .collect();
        let stake_merkle = MerkleTree::new(&hashed_nodes[..], true);
        get_proof(&stake_merkle, index)
    }

    async fn cast_votes(
        fixture: &mut TestBuilder,
        test_ncn: &TestNcn,
        ncn_program_client: &mut NCNProgramClient,
        ncn_pubkey: Pubkey,
        merkle_root: [u8; 32],
        snapshot_hash: [u8; 32],
    ) -> TestResult<()> {
        let epoch = fixture.clock().await.epoch;
        for operator_root in test_ncn.operators.iter() {
            let operator = operator_root.operator_pubkey;

            ncn_program_client
                .do_cast_vote(
                    ncn_pubkey,
                    operator,
                    &operator_root.operator_admin,
                    merkle_root,
                    snapshot_hash,
                    epoch,
                )
                .await?;
        }

        Ok(())
    }

    async fn setup_voting_epoch(
        fixture: &mut TestBuilder,
        test_ncn: &mut TestNcn,
        ncn_program_client: &mut NCNProgramClient,
    ) -> TestResult<()> {
        // Initialize the epoch state - creates a new state for the current epoch
        fixture.add_epoch_state_for_test_ncn(test_ncn).await?;

        // Initialize the weight table - prepares the table that will track voting weights
        let clock = fixture.clock().await;
        let epoch = clock.epoch;
        ncn_program_client
            .do_full_initialize_weight_table(test_ncn.ncn_root.ncn_pubkey, epoch)
            .await?;

        // Take staking token weight snapshot, epoch snapshot, operator snapshots and vault snapshots.
        ncn_program_client
            .do_set_epoch_weights(test_ncn.ncn_root.ncn_pubkey, epoch)
            .await?;
        fixture.add_epoch_snapshot_to_test_ncn(test_ncn).await?;
        fixture.add_operator_snapshots_to_test_ncn(test_ncn).await?;
        fixture
            .add_vault_operator_delegation_snapshots_to_test_ncn(test_ncn)
            .await?;

        // Initialize the ballot box - creates the voting container for this epoch
        fixture.add_ballot_box_to_test_ncn(test_ncn).await?;

        Ok(())
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
        fixture
            .add_operators_to_test_ncn(&mut test_ncn, OPERATOR_COUNT, Some(operator_fee_bps))
            .await?;

        // 2.c. Initialize the vaults using the Vault program By Jito
        // and initiate the handshake relationship between the NCN <> vaults, and vaults <> operators
        fixture
            .add_vaults_to_test_ncn(&mut test_ncn, 1, Some(mint.insecure_clone()))
            .await?;

        // 2.d. Vaults delegate stakes to operators
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

        // 4. Prepare the voting environment
        let epoch1 = fixture.clock().await.epoch;
        setup_voting_epoch(&mut fixture, &mut test_ncn, &mut ncn_program_client).await?;
        cast_votes(
            &mut fixture,
            &test_ncn,
            &mut ncn_program_client,
            ncn_pubkey,
            meta_merkle_snapshot.root,
            hash(b"snapshot1").to_bytes(),
        )
        .await?;

        // 6. Verify for some vote accounts
        for i in 0..10 {
            let bundle = &meta_merkle_snapshot.leaf_bundles[i * 10];

            let meta_proof = bundle.proof.clone().unwrap();
            let meta_leaf = &bundle.meta_merkle_leaf;

            ncn_program_client
                .do_verify_merkle(
                    ncn_pubkey,
                    epoch1,
                    meta_proof.clone(),
                    meta_leaf.clone(),
                    None,
                    None,
                )
                .await?;

            // Verify for stake accounts under this vote account.
            let stake_leaves = &bundle.stake_merkle_leaves;
            for (j, stake_leaf) in stake_leaves.iter().take(5).enumerate() {
                let stake_proof = get_stake_merkle_proof(stake_leaves.clone(), j);
                ncn_program_client
                    .do_verify_merkle(
                        ncn_pubkey,
                        epoch1,
                        meta_proof.clone(),
                        meta_leaf.clone(),
                        Some(stake_proof),
                        Some(stake_leaf.clone()),
                    )
                    .await?;
            }
        }

        // 7. Expect failure for wrong proofs.
        for i in 0..10 {
            let bundle1 = &meta_merkle_snapshot.leaf_bundles[i + 5];
            let bundle2 = &meta_merkle_snapshot.leaf_bundles[i + 3];

            let meta_leaf = &bundle1.meta_merkle_leaf;
            let wrong_proof = bundle2.proof.clone().unwrap();

            let result = ncn_program_client
                .do_verify_merkle(
                    ncn_pubkey,
                    epoch1,
                    wrong_proof,
                    meta_leaf.clone(),
                    None,
                    None,
                )
                .await;
            assert_ncn_program_error(result, NCNProgramError::InvalidProof, Some(0));

            let stake_leaves = &bundle1.stake_merkle_leaves;
            let proof = bundle1.proof.clone().unwrap();

            for (j, stake_leaf) in stake_leaves.iter().take(5).enumerate() {
                let mut stake_proof = get_stake_merkle_proof(stake_leaves.clone(), j);
                stake_proof.remove(0);
                let result = ncn_program_client
                    .do_verify_merkle(
                        ncn_pubkey,
                        epoch1,
                        proof.clone(),
                        meta_leaf.clone(),
                        Some(stake_proof),
                        Some(stake_leaf.clone()),
                    )
                    .await;
                assert_ncn_program_error(result, NCNProgramError::InvalidProof, Some(0));
            }
        }

        // Test invalid consensus result
        let bundle = &meta_merkle_snapshot.leaf_bundles[0];
        let meta_proof = bundle.proof.clone().unwrap();
        let meta_leaf = bundle.meta_merkle_leaf.clone();
        let transaction_error = ncn_program_client
            .do_verify_merkle(ncn_pubkey, epoch1 - 2, meta_proof, meta_leaf, None, None)
            .await;
        assert_ix_error(transaction_error, InstructionError::ProgramFailedToComplete);

        Ok(())
    }
}
