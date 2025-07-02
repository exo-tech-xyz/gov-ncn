#[cfg(test)]
mod tests {
    use jito_restaking_core::{config::Config, ncn_vault_ticket::NcnVaultTicket};
    use ncn_program_core::{
        ballot_box::Ballot,
        constants::{MAX_OPERATORS, WEIGHT},
        error::NCNProgramError,
    };
    use rand::Rng;
    use solana_sdk::{hash::hash, msg, signature::Keypair, signer::Signer};

    use crate::fixtures::{
        ncn_program_client::assert_ncn_program_error, test_builder::TestBuilder, TestResult,
    };

    #[tokio::test]
    async fn test_cast_vote() -> TestResult<()> {
        let mut fixture = TestBuilder::new().await;
        let mut ncn_program_client = fixture.ncn_program_client();

        let test_ncn = fixture.create_initial_test_ncn(1, 1, None).await?;

        ///// NCNProgram Setup /////
        fixture.warp_slot_incremental(1000).await?;

        fixture.snapshot_test_ncn(&test_ncn).await?;
        //////

        let clock = fixture.clock().await;
        let slot = clock.slot;
        let ncn = test_ncn.ncn_root.ncn_pubkey;
        let operator = test_ncn.operators[0].operator_pubkey;
        let epoch = clock.epoch;

        ncn_program_client
            .do_full_initialize_ballot_box(ncn, epoch)
            .await?;

        let winning_merkle = hash(b"root1").to_bytes();
        let winning_snapshot = hash(b"snapshot1").to_bytes();

        let operator_admin = &test_ncn.operators[0].operator_admin;

        ncn_program_client
            .do_cast_vote(
                ncn,
                operator,
                operator_admin,
                winning_merkle,
                winning_snapshot,
                epoch,
            )
            .await?;

        let ballot_box = ncn_program_client.get_ballot_box(ncn, epoch).await?;

        assert!(ballot_box.has_ballot(&Ballot::new(winning_merkle, winning_snapshot)));
        assert_eq!(ballot_box.slot_consensus_reached(), slot);
        assert!(ballot_box.is_consensus_reached());

        Ok(())
    }

    #[tokio::test]
    async fn test_operator_cannot_vote_twice() -> TestResult<()> {
        let mut fixture = TestBuilder::new().await;
        let mut ncn_program_client = fixture.ncn_program_client();

        let test_ncn = fixture.create_initial_test_ncn(3, 1, None).await?;

        ///// NCNProgram Setup /////
        fixture.warp_slot_incremental(1000).await?;
        fixture.snapshot_test_ncn(&test_ncn).await?;
        //////

        let clock = fixture.clock().await;
        let ncn = test_ncn.ncn_root.ncn_pubkey;
        let operator = test_ncn.operators[0].operator_pubkey;
        let operator_admin = &test_ncn.operators[0].operator_admin;
        let epoch = clock.epoch;

        // Initialize ballot box
        ncn_program_client
            .do_full_initialize_ballot_box(ncn, epoch)
            .await?;

        // First vote should succeed
        let merkle1 = hash(b"root1").to_bytes();
        let snapshot1 = hash(b"snapshot1").to_bytes();
        ncn_program_client
            .do_cast_vote(ncn, operator, operator_admin, merkle1, snapshot1, epoch)
            .await?;

        // Verify first vote was recorded
        let ballot_box = ncn_program_client.get_ballot_box(ncn, epoch).await?;
        assert!(ballot_box.has_ballot(&Ballot::new(merkle1, snapshot1)));
        assert_eq!(ballot_box.operators_voted(), 1);
        assert_eq!(ballot_box.unique_ballots(), 1);

        // Second vote should fail
        let merkle2 = hash(b"root2").to_bytes();
        let snapshot2 = hash(b"snapshot2").to_bytes();
        let result = ncn_program_client
            .do_cast_vote(ncn, operator, operator_admin, merkle2, snapshot2, epoch)
            .await;

        msg!("result: {:?}", result);
        assert_ncn_program_error(result, NCNProgramError::OperatorAlreadyVoted, Some(1));

        // Verify ballot box state remains unchanged
        let ballot_box = ncn_program_client.get_ballot_box(ncn, epoch).await?;
        assert!(ballot_box.has_ballot(&Ballot::new(merkle1, snapshot1)));
        assert!(!ballot_box.has_ballot(&Ballot::new(merkle2, snapshot2)));
        assert_eq!(ballot_box.operators_voted(), 1);
        assert_eq!(ballot_box.unique_ballots(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_bad_ballot() -> TestResult<()> {
        let mut fixture = TestBuilder::new().await;
        let mut ncn_program_client = fixture.ncn_program_client();

        let test_ncn = fixture.create_initial_test_ncn(3, 1, None).await?;

        ///// NCNProgram Setup /////
        fixture.warp_slot_incremental(1000).await?;

        fixture.snapshot_test_ncn(&test_ncn).await?;
        //////

        let clock = fixture.clock().await;
        let ncn = test_ncn.ncn_root.ncn_pubkey;
        let operator = test_ncn.operators[0].operator_pubkey;
        let epoch = clock.epoch;

        ncn_program_client
            .do_full_initialize_ballot_box(ncn, epoch)
            .await?;

        let operator_admin = &test_ncn.operators[0].operator_admin;

        // Vote with invalid root and hash.
        let result = ncn_program_client
            .do_cast_vote(ncn, operator, operator_admin, [0; 32], [0; 32], epoch)
            .await;

        assert_ncn_program_error(result, NCNProgramError::BadBallot, Some(1));

        Ok(())
    }

    // TODO: Failing with OperatorAlreadyVoted
    #[ignore = "long test"]
    #[tokio::test]
    async fn test_cast_vote_max_cu() -> TestResult<()> {
        let mut fixture = TestBuilder::new().await;
        let mut ncn_program_client = fixture.ncn_program_client();

        let test_ncn = fixture
            .create_initial_test_ncn(MAX_OPERATORS, 1, None)
            .await?;

        ///// NCNProgram Setup /////
        fixture.warp_slot_incremental(1000).await?;

        fixture.snapshot_test_ncn(&test_ncn).await?;
        //////

        let clock = fixture.clock().await;
        let ncn = test_ncn.ncn_root.ncn_pubkey;
        let epoch = clock.epoch;

        ncn_program_client
            .do_full_initialize_ballot_box(ncn, epoch)
            .await?;

        for operator in test_ncn.operators {
            let operator_admin = &operator.operator_admin;
            let i = rand::rng().random_range(0..=2);

            ncn_program_client
                .do_cast_vote(
                    ncn,
                    operator.operator_pubkey,
                    operator_admin,
                    [i; 32],
                    [i; 32],
                    epoch,
                )
                .await?;

            let ballot_box = ncn_program_client.get_ballot_box(ncn, epoch).await?;
            assert!(ballot_box.has_ballot(&Ballot::new([i; 32], [i; 32])));
        }

        let ballot_box = ncn_program_client.get_ballot_box(ncn, epoch).await?;
        msg!("ballot_box: {}", ballot_box);
        assert!(!ballot_box.is_consensus_reached());

        Ok(())
    }

    #[tokio::test]
    async fn test_zero_delegation_operator_cannot_vote() -> TestResult<()> {
        let mut fixture = TestBuilder::new().await;
        fixture.initialize_restaking_and_vault_programs().await?;

        let mut ncn_program_client = fixture.ncn_program_client();
        let mut restaking_client = fixture.restaking_program_client();

        const OPERATOR_COUNT: usize = 1;
        let mints = vec![(Keypair::new(), WEIGHT)];

        let mut test_ncn = fixture.create_test_ncn().await?;
        let ncn_pubkey = test_ncn.ncn_root.ncn_pubkey;

        fixture
            .add_operators_to_test_ncn(&mut test_ncn, OPERATOR_COUNT, Some(100))
            .await?;

        fixture
            .add_vaults_to_test_ncn(&mut test_ncn, 1, Some(mints[0].0.insecure_clone()))
            .await?;

        {
            // Fast-forward time to simulate a full epoch passing
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
        }

        // Setting up the NCN-program
        {
            ncn_program_client
                .do_initialize_config(test_ncn.ncn_root.ncn_pubkey, &test_ncn.ncn_root.ncn_admin)
                .await?;

            ncn_program_client
                .do_full_initialize_vault_registry(test_ncn.ncn_root.ncn_pubkey)
                .await?;

            for (mint, weight) in mints.iter() {
                ncn_program_client
                    .do_admin_register_st_mint(ncn_pubkey, mint.pubkey(), *weight)
                    .await?;
            }

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

        // Prepare the voting environment
        {
            fixture.add_epoch_state_for_test_ncn(&test_ncn).await?;
            let clock = fixture.clock().await;
            let epoch = clock.epoch;
            ncn_program_client
                .do_full_initialize_weight_table(test_ncn.ncn_root.ncn_pubkey, epoch)
                .await?;

            ncn_program_client
                .do_set_epoch_weights(test_ncn.ncn_root.ncn_pubkey, epoch)
                .await?;
            fixture.add_epoch_snapshot_to_test_ncn(&test_ncn).await?;
            fixture
                .add_operator_snapshots_to_test_ncn(&test_ncn)
                .await?;
            fixture
                .add_vault_operator_delegation_snapshots_to_test_ncn(&test_ncn)
                .await?;

            fixture.add_ballot_box_to_test_ncn(&test_ncn).await?;
        }

        // Cast votes
        {
            let epoch = fixture.clock().await.epoch;

            let zero_delegation_operator = test_ncn.operators.first().unwrap(); // Operator with no delegations

            // Vote from zero_delegation_operator (should fail with an error since operators with zero delegations cannot vote)
            // Verify the operator has no delegations by checking its snapshot
            let operator_snapshot = ncn_program_client
                .get_operator_snapshot(zero_delegation_operator.operator_pubkey, ncn_pubkey, epoch)
                .await?;

            // Log the current stake weight of the zero delegation operator
            let stake_weight = operator_snapshot.stake_weights().stake_weight();
            msg!("Zero-delegation operator stake weight: {}", stake_weight);

            // Confirm it has zero stake weight
            assert_eq!(
                stake_weight, 0,
                "Zero-delegation operator should have zero stake weight"
            );

            // We expect this to fail since the operator has zero delegations
            let result = ncn_program_client
                .do_cast_vote(
                    ncn_pubkey,
                    zero_delegation_operator.operator_pubkey,
                    &zero_delegation_operator.operator_admin,
                    hash(b"root1").to_bytes(),
                    hash(b"root1").to_bytes(),
                    epoch,
                )
                .await;

            // Verify that voting with zero delegation returns an error
            assert_ncn_program_error(result, NCNProgramError::CannotVoteWithZeroStake, Some(1));
        }

        Ok(())
    }
}
