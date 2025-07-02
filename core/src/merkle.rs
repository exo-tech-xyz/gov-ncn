use borsh::{BorshDeserialize, BorshSerialize};
use solana_sdk::{
    entrypoint::ProgramResult,
    hash::{hashv, Hash},
    pubkey::Pubkey,
};

use crate::error::NCNProgramError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorshHash(pub Hash);

impl BorshSerialize for BorshHash {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(self.0.as_ref())
    }
}

impl BorshDeserialize for BorshHash {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf)?;
        Ok(BorshHash(Hash::new_from_array(buf)))
    }
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct MetaMerkleSnapshot {
    /// Hash of MetaMerkleTree
    pub root: BorshHash,
    /// Each bundle contains the meta-level leaf, its stake-level leaves, and proof.
    pub leaf_bundles: Vec<MetaMerkleLeafBundle>,
    /// Slot where the tree was generated.
    pub slot: u64,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct MetaMerkleLeafBundle {
    /// MetaMerkleLeaf constructed from the StakeMerkleTree.
    pub meta_merkle_leaf: MetaMerkleLeaf,
    /// Leaf nodes of the StakeMerkleTree.
    pub stake_merkle_leaves: Vec<StakeMerkleLeaf>,
    /// Proof to verify MetaMerkleLeaf existence in MetaMerkleTree.
    pub proof: Option<Vec<[u8; 32]>>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct StakeMerkleLeaf {
    /// Wallet designated for governance voting for the stake account.
    pub voting_wallet: Pubkey,
    /// The stake account address.
    pub stake_account: Pubkey,
    /// Active delegated stake amount.
    pub active_stake: u64,
}

impl StakeMerkleLeaf {
    pub fn hash(&self) -> Hash {
        hashv(&[
            &self.voting_wallet.to_bytes(),
            &self.stake_account.to_bytes(),
            &self.active_stake.to_le_bytes(),
        ])
    }
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct MetaMerkleLeaf {
    /// Wallet designated for governance voting for the vote account.
    pub voting_wallet: Pubkey,
    /// Validator's vote account.
    pub vote_account: Pubkey,
    /// Root hash of the StakeMerkleTree, representing all active stake accounts
    /// delegated to the current vote account.
    pub stake_merkle_root: BorshHash,
    /// Total active delegated stake under this vote account.
    pub active_stake: u64,
}

impl MetaMerkleLeaf {
    pub fn hash(&self) -> Hash {
        hashv(&[
            &self.voting_wallet.to_bytes(),
            &self.vote_account.to_bytes(),
            &self.stake_merkle_root.0.to_bytes(),
            &self.active_stake.to_le_bytes(),
        ])
    }
}

macro_rules! hash_leaf {
    {$d:ident} => {
        hashv(&[LEAF_PREFIX, $d])
    }
}

macro_rules! hash_intermediate {
    {$l:ident, $r:ident} => {
        hashv(&[INTERMEDIATE_PREFIX, $l.as_ref(), $r.as_ref()])
    }
}

// We need to discern between leaf and intermediate nodes to prevent trivial second
// pre-image attacks.
// https://flawed.net.nz/2018/02/21/attacking-merkle-trees-with-a-second-preimage-attack
const LEAF_PREFIX: &[u8] = &[0];
const INTERMEDIATE_PREFIX: &[u8] = &[1];

/// Verifies a Merkle proof from a leaf's content and its sibling hashes.
///
/// Hashes the leaf with a prefix, then hashes iteratively with sorted sibling,
/// to reconstruct the parent root.
///
/// Compares the Merkle root with the expected `root` and returns an error if it doesnt match.
pub fn verify_helper(leaf_content: &[u8], proof_vec: Vec<[u8; 32]>, root: Hash) -> ProgramResult {
    let mut node = hash_leaf!(leaf_content);

    for proof in proof_vec {
        let sibling_node = Hash::from(proof);

        if node <= sibling_node {
            node = hash_intermediate!(node, sibling_node)
        } else {
            node = hash_intermediate!(sibling_node, node)
        }
    }

    if root != node {
        return Err(NCNProgramError::InvalidProof.into());
    }

    Ok(())
}
