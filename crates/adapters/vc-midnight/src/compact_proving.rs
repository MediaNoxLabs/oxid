// SPDX-License-Identifier: Apache-2.0

//! Exact Rust construction of the generated Compact presentation circuit's
//! `ProofPreimage`.
//!
//! The field-aligned input order, witness transcript, public state transcript,
//! output, binding input, and communications commitment mirror Compact runtime
//! 0.15.0. This module does not prove by itself; it is the deterministic seam
//! between credential-family validation and the separately configured native
//! proving runtime.

use std::borrow::Cow;

use midnight_base_crypto::fab::AlignedValue;
use midnight_transient_crypto::{
    curve::{EmbeddedGroupAffine, Fr},
    fab::{AlignedValueExt as _, ValueReprAlignedValue},
    hash::transient_hash,
    proofs::{KeyLocation, ProofPreimage},
    repr::FieldRepr as _,
};

use crate::{
    compact_digital_passport::{CompactCredential, CompactProof, VerificationMethodRef},
    compact_presentation::{CompactPresentationPublicInput, PublicDisclosures},
    digital_passport::PrivateParts,
};

pub(crate) const PRESENTATION_CIRCUIT_KEY_LOCATION: &str = "oxid-digital-passport-presentation-v1";

pub(crate) fn presentation_preimage(
    credential: &CompactCredential,
    credential_proof: &CompactProof,
    public_input: &CompactPresentationPublicInput,
    presentation_proof: &CompactProof,
    private_parts: &PrivateParts,
) -> ProofPreimage {
    presentation_preimage_with_key_location(
        credential,
        credential_proof,
        public_input,
        presentation_proof,
        private_parts,
        PRESENTATION_CIRCUIT_KEY_LOCATION,
    )
}

fn presentation_preimage_with_key_location(
    credential: &CompactCredential,
    credential_proof: &CompactProof,
    public_input: &CompactPresentationPublicInput,
    presentation_proof: &CompactProof,
    private_parts: &PrivateParts,
    key_location: &'static str,
) -> ProofPreimage {
    let input = concat(vec![
        credential_value(credential),
        proof_value(credential_proof),
        request_value(credential, public_input),
        presentation_value(credential, public_input.disclosures),
        proof_value(presentation_proof),
        AlignedValue::from(public_input.current_day),
        AlignedValue::from(public_input.verifier_domain_hash),
    ]);
    let output = AlignedValue::from(public_input.statement);
    let inputs = ValueReprAlignedValue(input.clone()).field_vec();

    let mut private_transcript = Vec::new();
    AlignedValue::from(private_parts.values.date_of_birth_days)
        .value_only_field_repr(&mut private_transcript);
    AlignedValue::from(private_parts.openings.date_of_birth)
        .value_only_field_repr(&mut private_transcript);

    let public_transcript_inputs = presentation_public_transcript(public_input.statement);
    // Only `popeq` query results belong here. This circuit's transcript is two
    // pushes followed by an insert, so it has no public transcript outputs.
    let public_transcript_outputs = Vec::new();

    // Compact runtime 0.15.0 constructs its communications commitment as
    // transientHash([0, ...inputValueFields, ...outputValueFields]) with zero
    // randomness. The value-only input representation is deliberately not the
    // alignment-bearing `inputs` vector consumed by the prover.
    let mut communications_preimage = Vec::new();
    communications_preimage.push(Fr::from(0_u64));
    input.value_only_field_repr(&mut communications_preimage);
    output.value_only_field_repr(&mut communications_preimage);

    ProofPreimage {
        inputs,
        private_transcript,
        public_transcript_inputs,
        public_transcript_outputs,
        binding_input: Fr::from(0_u64),
        communications_commitment: Some((
            transient_hash(&communications_preimage),
            Fr::from(0_u64),
        )),
        key_location: KeyLocation(Cow::Borrowed(key_location)),
    }
}

fn credential_value(credential: &CompactCredential) -> AlignedValue {
    concat(vec![
        AlignedValue::from(credential.version),
        schema_value(credential),
        method_value(credential.issuer),
        method_value(credential.holder),
        AlignedValue::from(credential.issued_at),
        AlignedValue::from(credential.has_expiration),
        AlignedValue::from(credential.expires_at),
        AlignedValue::from(credential.commitments.first_name),
        AlignedValue::from(credential.commitments.last_name),
        AlignedValue::from(credential.commitments.date_of_birth),
        AlignedValue::from(credential.commitments.document_number),
        AlignedValue::from(credential.commitments.issuing_state),
        AlignedValue::from(credential.commitments.claim_root),
    ])
}

fn schema_value(credential: &CompactCredential) -> AlignedValue {
    concat(vec![
        AlignedValue::from(credential.package_id),
        AlignedValue::from(credential.schema_id),
        AlignedValue::from(credential.major_version),
        AlignedValue::from(credential.minor_version),
    ])
}

fn method_value(method: VerificationMethodRef) -> AlignedValue {
    concat(vec![
        AlignedValue::from(method.did_contract_address),
        AlignedValue::from(method.method_id),
    ])
}

fn proof_value(proof: &CompactProof) -> AlignedValue {
    concat(vec![
        method_value(proof.signer),
        AlignedValue::from(proof.created_at),
        AlignedValue::from(proof.challenge_hash),
        point_value(proof.public_key),
        point_value(proof.announcement),
        AlignedValue::from(proof.response),
    ])
}

fn point_value(point: EmbeddedGroupAffine) -> AlignedValue {
    AlignedValue::from(point)
}

fn request_value(
    credential: &CompactCredential,
    public_input: &CompactPresentationPublicInput,
) -> AlignedValue {
    let disclosure = public_input.disclosures;
    concat(vec![
        AlignedValue::from(credential.version),
        schema_value(credential),
        method_value(credential.issuer),
        AlignedValue::from(disclosure.reveal_first_name),
        AlignedValue::from(disclosure.reveal_last_name),
        AlignedValue::from(disclosure.prove_age),
        AlignedValue::from(disclosure.age_threshold_years),
        AlignedValue::from(disclosure.reveal_document_number),
        AlignedValue::from(disclosure.reveal_issuing_state),
        AlignedValue::from(public_input.verifier_challenge_hash),
    ])
}

fn presentation_value(
    credential: &CompactCredential,
    disclosure: PublicDisclosures,
) -> AlignedValue {
    concat(vec![
        AlignedValue::from(credential.version),
        schema_value(credential),
        AlignedValue::from(credential.commitments.claim_root),
        method_value(credential.issuer),
        method_value(credential.holder),
        disclosures_value(disclosure),
    ])
}

fn disclosures_value(disclosure: PublicDisclosures) -> AlignedValue {
    concat(vec![
        AlignedValue::from(disclosure.reveal_first_name),
        AlignedValue::from(disclosure.first_name),
        AlignedValue::from(disclosure.first_name_opening),
        AlignedValue::from(disclosure.reveal_last_name),
        AlignedValue::from(disclosure.last_name),
        AlignedValue::from(disclosure.last_name_opening),
        AlignedValue::from(disclosure.prove_age),
        AlignedValue::from(disclosure.age_threshold_years),
        AlignedValue::from(disclosure.reveal_document_number),
        AlignedValue::from(disclosure.document_number),
        AlignedValue::from(disclosure.document_number_opening),
        AlignedValue::from(disclosure.reveal_issuing_state),
        AlignedValue::from(disclosure.issuing_state),
        AlignedValue::from(disclosure.issuing_state_opening),
    ])
}

pub(crate) fn presentation_public_transcript(statement: [u8; 32]) -> Vec<Fr> {
    let mut output = Vec::new();
    push_cell_operation(false, AlignedValue::from(1_u8), &mut output);
    push_cell_operation(true, AlignedValue::from(statement), &mut output);
    // `ins { cached: false, n: 1 }` is encoded as 0x90 | n.
    output.push(Fr::from(0x91_u64));
    output
}

fn push_cell_operation(storage: bool, value: AlignedValue, output: &mut Vec<Fr>) {
    // `push` is 0x10 and storage adds one; StateValue::Cell is tagged one.
    output.push(Fr::from(0x10_u64 + u64::from(storage)));
    output.push(Fr::from(1_u64));
    value.field_repr(output);
}

fn concat(parts: Vec<AlignedValue>) -> AlignedValue {
    AlignedValue::concat(parts.iter())
}

#[cfg(test)]
mod tests {
    use midnight_serialize::tagged_serialize;
    use midnight_transient_crypto::curve::EmbeddedFr;
    use oxid_presentation_domain::RequestedPresentationClaim;
    use sha2::{Digest as _, Sha256};

    use super::*;
    use crate::{
        compact_digital_passport::{encode_proof, parse_credential, parse_proof},
        compact_presentation::{
            prepare_public_input, presentation_proof_challenge, verify_presentation_proof,
        },
        digital_passport::{
            CLAIM_DATE_OF_BIRTH, CLAIM_FIRST_NAME, CLAIM_LAST_NAME, validated_private_parts,
        },
        standalone_compact_credential, standalone_compact_proof, standalone_private_material,
    };

    fn requested_claims() -> Vec<RequestedPresentationClaim> {
        vec![
            RequestedPresentationClaim::reveal(CLAIM_FIRST_NAME, "First name").expect("claim"),
            RequestedPresentationClaim::reveal(CLAIM_LAST_NAME, "Last name").expect("claim"),
            RequestedPresentationClaim::predicate(
                CLAIM_DATE_OF_BIRTH,
                "Age over 18",
                "age_over",
                18,
            )
            .expect("claim"),
        ]
    }

    #[test]
    fn matches_generated_compact_runtime_proof_preimage_byte_for_byte() {
        let credential_bytes = standalone_compact_credential();
        let issuer_proof_bytes = standalone_compact_proof();
        let private_material = standalone_private_material();
        let credential = parse_credential(&credential_bytes).expect("credential");
        let issuer_proof = parse_proof(&issuer_proof_bytes).expect("issuer proof");
        let (_, private_parts) =
            validated_private_parts(&credential_bytes, &private_material).expect("private parts");
        let public_input = prepare_public_input(
            &credential_bytes,
            &private_material,
            [0x11; 32],
            [0x22; 32],
            20_000,
            &requested_claims(),
        )
        .expect("public input");

        let secret = EmbeddedFr::from(987_654_321_u64);
        let nonce = EmbeddedFr::from(17_u64);
        let mut holder_proof = CompactProof {
            signer: credential.holder,
            created_at: 10_100,
            challenge_hash: [0x11; 32],
            public_key: EmbeddedGroupAffine::generator() * secret,
            announcement: EmbeddedGroupAffine::generator() * nonce,
            response: Fr::from(0_u64),
        };
        let challenge = EmbeddedFr::try_from(presentation_proof_challenge(
            public_input.presentation_root,
            &holder_proof,
        ))
        .expect("embedded challenge");
        holder_proof.response =
            Fr::from_le_bytes(&(nonce + challenge * secret).as_le_bytes()).expect("response field");
        assert_eq!(
            hex::encode(public_input.statement),
            "475caef55fc4b454931beb6b4435688ed36cc1740d33ade45741dcd31214011c"
        );
        assert_eq!(
            hex::encode(public_input.presentation_root),
            "cf7570efcabe17ba6aa6920aed951f2794a7d609a03a49920694c5c4e09d2876"
        );
        assert!(verify_presentation_proof(
            public_input.presentation_root,
            &holder_proof
        ));
        assert!(parse_proof(&encode_proof(&holder_proof).expect("holder proof")).is_ok());

        let preimage = presentation_preimage(
            &credential,
            &issuer_proof,
            &public_input,
            &holder_proof,
            &private_parts,
        );
        let mut encoded = Vec::new();
        tagged_serialize(&preimage, &mut encoded).expect("tagged preimage");

        assert_eq!(encoded.len(), 1_506);
        assert_eq!(
            hex::encode(Sha256::digest(&encoded)),
            "5f0618c1ef46d61aa3a9848907ca46a6ea5ac8bb75714b1baa9f3f2b6d32830a"
        );
        assert_eq!(preimage.inputs.len(), 117);
        assert_eq!(preimage.private_transcript.len(), 3);
        assert_eq!(preimage.public_transcript_inputs.len(), 12);
        assert!(preimage.public_transcript_outputs.is_empty());
    }
}
