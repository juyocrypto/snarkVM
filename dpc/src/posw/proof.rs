// Copyright (C) 2019-2021 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use crate::Network;
use snarkvm_algorithms::SNARK;
use snarkvm_utilities::{
    fmt,
    io::{Read, Result as IoResult, Write},
    str::FromStr,
    FromBytes,
    FromBytesVisitor,
    ToBytes,
    ToBytesSerializer,
};

use anyhow::{anyhow, Result};
use serde::{de, ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

/// A wrapper enum for a PoSW proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoSWProof<N: Network> {
    NonHiding(N::PoSWProof),
    Hiding(crate::testnet2::DeprecatedPoSWProof<N>),
}

impl<N: Network> PoSWProof<N> {
    /// Initializes a new instance of a PoSW proof.
    pub fn new(proof: N::PoSWProof) -> Self {
        Self::NonHiding(proof)
    }

    /// Initializes a new instance of a hiding PoSW proof.
    pub fn new_hiding(proof: crate::testnet2::DeprecatedPoSWProof<N>) -> Self {
        Self::Hiding(proof)
    }

    /// Returns `true` if the PoSW proof is hiding.
    pub fn is_hiding(&self) -> bool {
        match self {
            Self::NonHiding(..) => false,
            Self::Hiding(..) => true,
        }
    }

    /// Returns `true` if the PoSW proof is valid.
    pub fn verify(
        &self,
        verifying_key: &<<N as Network>::PoSWSNARK as SNARK>::VerifyingKey,
        inputs: &Vec<N::InnerScalarField>,
    ) -> bool {
        match self {
            Self::NonHiding(proof) => {
                // Ensure the proof is valid.
                if !<<N as Network>::PoSWSNARK as SNARK>::verify(verifying_key, inputs, proof).unwrap() {
                    eprintln!("PoSW proof verification failed");
                    return false;
                }
            }
            Self::Hiding(proof) => {
                let verifying_key =
                    match <crate::testnet2::DeprecatedPoSWSNARK<N> as SNARK>::VerifyingKey::from_bytes_le(
                        &verifying_key.to_bytes_le().unwrap(),
                    ) {
                        Ok(vk) => vk,
                        Err(error) => {
                            eprintln!("Failed to read deprecated PoSW VK from bytes: {}", error);
                            return false;
                        }
                    };

                // Ensure the proof is valid.
                if !<crate::testnet2::DeprecatedPoSWSNARK<N> as SNARK>::verify(&verifying_key, inputs, proof).unwrap() {
                    eprintln!("[deprecated] PoSW proof verification failed");
                    return false;
                }
            }
        }

        true
    }

    /// Returns the PoSW proof size in bytes.
    pub fn size(&self) -> usize {
        match self {
            Self::NonHiding(..) => N::HEADER_PROOF_SIZE_IN_BYTES,
            Self::Hiding(..) => 771,
        }
    }
}

impl<N: Network> FromBytes for PoSWProof<N> {
    #[inline]
    fn read_le<R: Read>(reader: R) -> IoResult<Self> {
        Ok(bincode::deserialize_from(reader).expect("FAILED"))
    }
}

impl<N: Network> ToBytes for PoSWProof<N> {
    #[inline]
    fn write_le<W: Write>(&self, writer: W) -> IoResult<()> {
        match self {
            Self::NonHiding(proof) => proof.write_le(writer),
            Self::Hiding(proof) => proof.write_le(writer),
        }
    }
}

impl<N: Network> FromStr for PoSWProof<N> {
    type Err = anyhow::Error;

    fn from_str(header: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(&header)?)
    }
}

impl<N: Network> fmt::Display for PoSWProof<N> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_string(self).map_err::<fmt::Error, _>(serde::ser::Error::custom)?
        )
    }
}

impl<N: Network> Serialize for PoSWProof<N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match serializer.is_human_readable() {
            true => {
                let mut header = serializer.serialize_struct("PoSWProof", 1)?;
                match self {
                    Self::NonHiding(proof) => header.serialize_field("non_hiding", proof)?,
                    Self::Hiding(proof) => header.serialize_field("hiding", proof)?,
                }
                header.end()
            }
            false => ToBytesSerializer::serialize(self, serializer),
        }
    }
}

impl<'de, N: Network> Deserialize<'de> for PoSWProof<N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match deserializer.is_human_readable() {
            true => {
                let proof = serde_json::Value::deserialize(deserializer)?;

                if let Ok(proof) = serde_json::from_value(proof["non_hiding"].clone()) {
                    Ok(Self::NonHiding(proof))
                } else if let Ok(proof) = serde_json::from_value(proof["hiding"].clone()) {
                    Ok(Self::Hiding(proof))
                } else {
                    Err(anyhow!("Invalid human-readable deserialization")).map_err(de::Error::custom)?
                }
            }
            false => {
                let mut buffer = Vec::with_capacity(771);
                deserializer.deserialize_tuple(771, FromBytesVisitor::new(&mut buffer, "PoSW proof"))?;

                if let Ok(proof) = N::PoSWProof::read_le(&buffer[..N::HEADER_PROOF_SIZE_IN_BYTES]) {
                    return Ok(Self::NonHiding(proof));
                }

                if let Ok(proof) = crate::testnet2::DeprecatedPoSWProof::<N>::read_le(&buffer[..]) {
                    return Ok(Self::Hiding(proof));
                }

                Err(anyhow!("Invalid byte deserialization")).map_err(de::Error::custom)?
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{testnet1::Testnet1, testnet2::Testnet2, Block};

    #[test]
    fn test_load_genesis_proof() {
        use snarkvm_parameters::Genesis;
        {
            let block =
                Block::<Testnet1>::read_le(&snarkvm_parameters::testnet1::GenesisBlock::load_bytes()[..]).unwrap();
            let proof = block.header().proof().to_owned().unwrap();
            assert_eq!(proof.to_bytes_le().unwrap().len(), Testnet1::HEADER_PROOF_SIZE_IN_BYTES);
        }
        {
            let block =
                Block::<Testnet2>::read_le(&snarkvm_parameters::testnet2::GenesisBlock::load_bytes()[..]).unwrap();
            let proof = block.header().proof().to_owned().unwrap();
            assert_eq!(proof.to_bytes_le().unwrap().len(), 771);
        }
    }

    #[test]
    fn test_proof_genesis_size() {
        let proof = Testnet1::genesis_block().header().proof().to_owned().unwrap();
        assert_eq!(proof.to_bytes_le().unwrap().len(), Testnet1::HEADER_PROOF_SIZE_IN_BYTES);
        assert_eq!(
            bincode::serialize(&proof).unwrap().len(),
            Testnet1::HEADER_PROOF_SIZE_IN_BYTES
        );

        let proof = Testnet2::genesis_block().header().proof().to_owned().unwrap();
        assert_eq!(proof.to_bytes_le().unwrap().len(), 771);
        assert_eq!(bincode::serialize(&proof).unwrap().len(), 771);
    }

    #[test]
    fn test_proof_serde_json() {
        {
            let proof = Testnet1::genesis_block().header().proof().to_owned().unwrap();

            // Serialize
            let expected_string = proof.to_string();
            let candidate_string = serde_json::to_string(&proof).unwrap();
            assert_eq!(1601, candidate_string.len(), "Update me if serialization has changed");
            assert_eq!(expected_string, candidate_string);

            // Deserialize
            assert_eq!(proof, PoSWProof::from_str(&candidate_string).unwrap());
            assert_eq!(proof, serde_json::from_str(&candidate_string).unwrap());
        }
        {
            let proof = Testnet2::genesis_block().header().proof().to_owned().unwrap();

            // Serialize
            let expected_string = proof.to_string();
            let candidate_string = serde_json::to_string(&proof).unwrap();
            assert_eq!(1601, candidate_string.len(), "Update me if serialization has changed");
            assert_eq!(expected_string, candidate_string);

            // Deserialize
            assert_eq!(proof, PoSWProof::from_str(&candidate_string).unwrap());
            assert_eq!(proof, serde_json::from_str(&candidate_string).unwrap());
        }
    }

    #[test]
    fn test_proof_bincode() {
        {
            let proof = Testnet1::genesis_block().header().proof().to_owned().unwrap();

            let expected_bytes = proof.to_bytes_le().unwrap();
            assert_eq!(&expected_bytes[..], &bincode::serialize(&proof).unwrap()[..]);

            assert_eq!(proof, PoSWProof::read_le(&expected_bytes[..]).unwrap());
            assert_eq!(proof, bincode::deserialize(&expected_bytes[..]).unwrap());
        }
        {
            let proof = Testnet2::genesis_block().header().proof().to_owned().unwrap();

            let expected_bytes = proof.to_bytes_le().unwrap();
            assert_eq!(&expected_bytes[..], &bincode::serialize(&proof).unwrap()[..]);

            assert_eq!(proof, PoSWProof::read_le(&expected_bytes[..]).unwrap());
            assert_eq!(proof, bincode::deserialize(&expected_bytes[..]).unwrap());
        }
    }
}
