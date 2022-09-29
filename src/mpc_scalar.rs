//! Groups the definitions and trait implementations for a scalar value within an MPC network
#![allow(unused_doc_comments)]
use std::{
    borrow::Borrow,
    convert::TryInto,
    iter::{Product, Sum},
    ops::{Add, AddAssign, Index, Mul, MulAssign, Neg, Sub, SubAssign},
};

use curve25519_dalek::{ristretto::RistrettoPoint, scalar::Scalar};
use futures::executor::block_on;
use rand_core::{CryptoRng, OsRng, RngCore};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::{
    beaver::SharedValueSource,
    commitment::PedersenCommitment,
    error::{MpcError, MpcNetworkError},
    macros,
    network::MpcNetwork,
    BeaverSource, SharedNetwork, Visibility, Visible,
};

/// Represents a scalar value allocated in an MPC network
#[derive(Debug)]
pub struct MpcScalar<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> {
    /// the underlying value of the scalar allocated in the network
    pub(crate) value: Scalar,
    /// The visibility flag; what amount of information parties have
    pub(crate) visibility: Visibility,
    /// The underlying network that the MPC operates on
    pub(crate) network: SharedNetwork<N>,
    /// The source for shared values; MAC keys, beaver triples, etc
    pub(crate) beaver_source: BeaverSource<S>,
}

impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Clone for MpcScalar<N, S> {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            visibility: self.visibility,
            network: self.network.clone(),
            beaver_source: self.beaver_source.clone(),
        }
    }
}

/**
 * Static and helper methods
 */

/// Converts a scalar to u64
pub fn scalar_to_u64(a: &Scalar) -> u64 {
    u64::from_le_bytes(a.to_bytes()[..8].try_into().unwrap()) as u64
}

/**
 * Wrapper type implementations
 */
impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> MpcScalar<N, S> {
    /**
     * Helper methods
     */
    #[inline]
    pub(crate) fn is_shared(&self) -> bool {
        self.visibility == Visibility::Shared
    }

    #[inline]
    pub(crate) fn is_public(&self) -> bool {
        self.visibility == Visibility::Public
    }

    #[inline]
    pub fn value(&self) -> Scalar {
        self.value
    }

    #[inline]
    pub fn to_scalar(&self) -> Scalar {
        self.value()
    }

    #[inline]
    pub(crate) fn network(&self) -> SharedNetwork<N> {
        self.network.clone()
    }

    #[inline]
    pub(crate) fn beaver_source(&self) -> BeaverSource<S> {
        self.beaver_source.clone()
    }

    /**
     * Casting methods
     */

    /// Create a public network scalar from a u64
    pub fn from_public_u64(
        a: u64,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Self {
        Self::from_u64_with_visibility(a, Visibility::Public, network, beaver_source)
    }

    /// Create a private network scalar from a given u64
    pub fn from_private_u64(
        a: u64,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Self {
        Self::from_u64_with_visibility(a, Visibility::Private, network, beaver_source)
    }

    /// Create a scalar from a given u64 and visibility
    pub(crate) fn from_u64_with_visibility(
        a: u64,
        visibility: Visibility,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Self {
        Self {
            network,
            visibility,
            beaver_source,
            value: Scalar::from(a),
        }
    }

    /// Allocate a public network value from an underlying scalar
    pub fn from_public_scalar(
        value: Scalar,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Self {
        Self::from_scalar_with_visibility(value, Visibility::Public, network, beaver_source)
    }

    /// Allocate a private network value from an underlying scalar
    pub fn from_private_scalar(
        value: Scalar,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Self {
        Self::from_scalar_with_visibility(value, Visibility::Private, network, beaver_source)
    }

    /// Allocate an existing scalar in the network with given visibility
    pub(crate) fn from_scalar_with_visibility(
        value: Scalar,
        visibility: Visibility,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Self {
        Self {
            network,
            visibility,
            value,
            beaver_source,
        }
    }

    /// Generate a random scalar
    /// Random will always be SharedWithOwner(self); two parties cannot reliably generate the same random value
    pub fn random<R: RngCore + CryptoRng>(
        rng: &mut R,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Self {
        Self {
            network,
            visibility: Visibility::Private,
            beaver_source,
            value: Scalar::random(rng),
        }
    }

    /// Default-esque implementation
    pub fn default(network: SharedNetwork<N>, beaver_source: BeaverSource<S>) -> Self {
        Self::zero(network, beaver_source)
    }

    // Build a scalar from bytes
    macros::impl_delegated_wrapper!(
        Scalar,
        from_bytes_mod_order,
        from_bytes_mod_order_with_visibility,
        bytes,
        [u8; 32]
    );
    macros::impl_delegated_wrapper!(
        Scalar,
        from_bytes_mod_order_wide,
        from_bytes_mod_order_wide_with_visibility,
        input,
        &[u8; 64]
    );

    pub fn from_canonical_bytes(
        bytes: [u8; 32],
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Option<MpcScalar<N, S>> {
        Self::from_canonical_bytes_with_visibility(
            bytes,
            Visibility::Public,
            network,
            beaver_source,
        )
    }

    pub fn from_canonical_bytes_with_visibility(
        bytes: [u8; 32],
        visibility: Visibility,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Option<MpcScalar<N, S>> {
        Some(MpcScalar {
            visibility,
            network,
            beaver_source,
            value: Scalar::from_canonical_bytes(bytes)?,
        })
    }

    macros::impl_delegated_wrapper!(
        Scalar,
        from_bits,
        from_bits_with_visibility,
        bytes,
        [u8; 32]
    );

    // Convert a scalar to bytes
    macros::impl_delegated!(to_bytes, self, [u8; 32]);
    macros::impl_delegated!(as_bytes, self, &[u8; 32]);
    // Check whether the scalar is canonically represented mod l
    macros::impl_delegated!(is_canonical, self, bool);
    // Generate the additive identity
    macros::impl_delegated_wrapper!(Scalar, zero);
    // Generate the multiplicative identity
    macros::impl_delegated_wrapper!(Scalar, one);
}

/**
 * Secret sharing implementation
 */
impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> MpcScalar<N, S> {
    /// From a privately held value, construct an additive secret share and distribute this
    /// to the counterparty. The local party samples a random value R which is given to the peer
    /// The local party then holds a - R where a is the underlying value.
    /// This method is called by both parties, only one of which transmits, the peer will simply
    /// await the sent share
    pub fn share_secret(&self, party_id: u64) -> Result<MpcScalar<N, S>, MpcNetworkError> {
        let my_party_id = self.network.as_ref().borrow().party_id();

        if my_party_id == party_id {
            // Sender party
            // Sample a random additive complement
            let mut rng = OsRng {};
            let random_share = Scalar::random(&mut rng);

            // Broadcast the counterparty's share
            block_on(
                self.network
                    .as_ref()
                    .borrow_mut()
                    .send_single_scalar(random_share),
            )?;

            // Do not subtract directly as the random scalar is not directly allocated in the network
            // subtracting directly ties it to the subtraction implementaiton in a fragile way
            Ok(MpcScalar {
                value: self.value - random_share,
                visibility: Visibility::Shared,
                network: self.network.clone(),
                beaver_source: self.beaver_source.clone(),
            })
        } else {
            Self::receive_value(self.network.clone(), self.beaver_source.clone())
        }
    }

    /// Share a batch of privately held secrets by constructing additive shares
    pub fn batch_share_secrets(
        party_id: u64,
        secrets: &[MpcScalar<N, S>],
    ) -> Result<Vec<MpcScalar<N, S>>, MpcNetworkError> {
        assert!(
            !secrets.is_empty(),
            "Cannot batch share an empty vector of values"
        );
        let network = secrets[0].network();
        let beaver_source = secrets[0].beaver_source();
        let my_party_id = network.as_ref().borrow().party_id();

        if my_party_id == party_id {
            // Sender party
            let mut rng = OsRng {};
            let random_shares: Vec<Scalar> = (0..secrets.len())
                .map(|_| Scalar::random(&mut rng))
                .collect();

            // Broadcast the random shares to the peer
            block_on(network.as_ref().borrow_mut().send_scalars(&random_shares))?;

            Ok(secrets
                .iter()
                .zip(random_shares.iter())
                .map(|(secret, blinding)| MpcScalar {
                    value: secret.value() - blinding,
                    visibility: Visibility::Shared,
                    network: network.clone(),
                    beaver_source: beaver_source.clone(),
                })
                .collect())
        } else {
            Self::receive_values_batch(secrets.len(), network, beaver_source)
        }
    }

    /// Local party receives a secret share of a value; as opposed to using share_secret, no existing value is needed
    pub fn receive_value(
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Result<MpcScalar<N, S>, MpcNetworkError> {
        let value = block_on(network.as_ref().borrow_mut().receive_single_scalar())?;

        Ok(MpcScalar {
            value,
            visibility: Visibility::Shared,
            network,
            beaver_source,
        })
    }

    /// Local party receives a batch of shared values
    pub fn receive_values_batch(
        num_expected: usize,
        network: SharedNetwork<N>,
        beaver_source: BeaverSource<S>,
    ) -> Result<Vec<MpcScalar<N, S>>, MpcNetworkError> {
        let values = block_on(network.as_ref().borrow_mut().receive_scalars(num_expected))?;

        Ok(values
            .iter()
            .map(|value| MpcScalar {
                value: *value,
                visibility: Visibility::Shared,
                network: network.clone(),
                beaver_source: beaver_source.clone(),
            })
            .collect())
    }

    /// From a shared value, both parties open their shares and construct the plaintext value.
    /// Note that the parties no longer hold valid additive secret shares of the value, this is used
    /// at the end of a computation
    pub fn open(&self) -> Result<MpcScalar<N, S>, MpcNetworkError> {
        if self.is_public() {
            return Ok(self.clone());
        }

        // Send my scalar and expect one back
        let received_scalar = block_on(
            self.network
                .as_ref()
                .borrow_mut()
                .broadcast_single_scalar(self.value),
        )?;

        // Reconstruct the plaintext from the peer's share
        Ok(MpcScalar::from_public_scalar(
            self.value + received_scalar,
            self.network.clone(),
            self.beaver_source.clone(),
        ))
    }

    /// Open a batch of shared values
    pub fn open_batch(values: &[MpcScalar<N, S>]) -> Result<Vec<MpcScalar<N, S>>, MpcNetworkError> {
        assert!(
            !values.is_empty(),
            "Cannot batch open an empty vector of values"
        );
        let network = values[0].network();
        let beaver_source = values[0].beaver_source();

        // Both parties share their values
        let received_scalars = block_on(
            network.as_ref().borrow_mut().broadcast_scalars(
                &values
                    .iter()
                    .map(|value| value.value())
                    .collect::<Vec<Scalar>>(),
            ),
        )?;

        Ok(values
            .iter()
            .zip(received_scalars.iter())
            .map(|(my_share, peer_share)| {
                MpcScalar::from_public_scalar(
                    my_share.value() + peer_share,
                    network.clone(),
                    beaver_source.clone(),
                )
            })
            .collect())
    }

    /// From a shared value:
    ///     1. Commit to the value and exchange commitments
    ///     2. Open those commitments to the underlying value
    ///     3. Verify that the peer's opening matches their commitment
    pub fn commit_and_open(&self) -> Result<MpcScalar<N, S>, MpcError> {
        // Only a shared value can be committed and opened
        if !self.is_shared() {
            return Err(MpcError::VisibilityError(
                "commit_and_open may only be called on shared values".to_string(),
            ));
        }

        // Compute a Pedersen commitment to the value
        let commitment = PedersenCommitment::commit(self.to_scalar());
        let peer_commitment = block_on(
            self.network()
                .as_ref()
                .borrow_mut()
                .broadcast_single_point(commitment.get_commitment()),
        )
        .map_err(MpcError::NetworkError)?;

        // Open the commitment to the underlying value
        let received_scalars = block_on(
            self.network()
                .as_ref()
                .borrow_mut()
                .broadcast_scalars(&[commitment.get_blinding(), commitment.get_value()]),
        )
        .map_err(MpcError::NetworkError)?;

        let (peer_blinding, peer_value) = (received_scalars[0], received_scalars[1]);

        // Verify the commitment and return the opened value
        if !PedersenCommitment::verify_from_values(peer_commitment, peer_blinding, peer_value) {
            return Err(MpcError::AuthenticationError);
        }

        Ok(Self {
            value: self.value() + peer_value,
            visibility: Visibility::Public,
            network: self.network(),
            beaver_source: self.beaver_source(),
        })
    }

    /// Commit to and open a batch of secret shared values
    pub fn batch_commit_and_open(
        values: &[MpcScalar<N, S>],
    ) -> Result<Vec<MpcScalar<N, S>>, MpcError> {
        assert!(
            !values.is_empty(),
            "Cannot batch commit and open an empty vector of values"
        );
        let network = values[0].network();
        let beaver_source = values[0].beaver_source();

        // Generate commitments to the values and share them with the peer
        let commitments: Vec<PedersenCommitment> = values
            .iter()
            .map(|value| PedersenCommitment::commit(value.to_scalar()))
            .collect();
        let peer_commitments = block_on(
            network.as_ref().borrow_mut().broadcast_points(
                &commitments
                    .iter()
                    .map(|comm| comm.get_commitment())
                    .collect::<Vec<RistrettoPoint>>(),
            ),
        )
        .map_err(MpcError::NetworkError)?;

        // Open both the underlying values and the blinding factos
        let mut commitment_data: Vec<Scalar> = Vec::new();
        commitments.iter().for_each(|comm| {
            commitment_data.push(comm.get_blinding());
            commitment_data.push(comm.get_value());
        });

        let received_values = block_on(
            network
                .as_ref()
                .borrow_mut()
                .broadcast_scalars(&commitment_data),
        )
        .map_err(MpcError::NetworkError)?;

        // Verify the peer's commitments
        let mut peer_values: Vec<Scalar> = Vec::new();
        received_values
            .chunks(2 /* chunk_size */) // Fetch each pair of blinding, value
            .zip(peer_commitments.into_iter())
            .try_for_each(|(revealed_values, comm)| {
                // Destructure the received payload and append to the peer values vector
                let (blinding, value) = (revealed_values[0], revealed_values[1]);
                peer_values.push(value);

                // Verify the Pedersen commitment, report an authentication error if opening fails
                if !PedersenCommitment::verify_from_values(comm, blinding, value) {
                    return Err(MpcError::AuthenticationError);
                }

                Ok(())
            })?;

        // If the commitments open properly then add shares together to recover cleartext
        Ok(values
            .iter()
            .zip(peer_values)
            .map(|(my_value, peer_value)| MpcScalar {
                value: my_value.value() + peer_value,
                visibility: Visibility::Public,
                network: network.clone(),
                beaver_source: beaver_source.clone(),
            })
            .collect())
    }

    /// Retreives the next Beaver triplet from the Beaver source and allocates the values within the network
    fn next_beaver_triplet(&self) -> (MpcScalar<N, S>, MpcScalar<N, S>, MpcScalar<N, S>) {
        let (a, b, c) = self.beaver_source.as_ref().borrow_mut().next_triplet();

        (
            MpcScalar::from_scalar_with_visibility(
                a,
                Visibility::Shared,
                self.network.clone(),
                self.beaver_source.clone(),
            ),
            MpcScalar::from_scalar_with_visibility(
                b,
                Visibility::Shared,
                self.network.clone(),
                self.beaver_source.clone(),
            ),
            MpcScalar::from_scalar_with_visibility(
                c,
                Visibility::Shared,
                self.network.clone(),
                self.beaver_source.clone(),
            ),
        )
    }
}

/**
 * Generic trait implementations
 */
impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Visible for MpcScalar<N, S> {
    fn visibility(&self) -> Visibility {
        self.visibility
    }
}

impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> PartialEq for MpcScalar<N, S> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> ConstantTimeEq for MpcScalar<N, S> {
    fn ct_eq(&self, other: &Self) -> subtle::Choice {
        self.value.ct_eq(&other.value)
    }
}

impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Index<usize> for MpcScalar<N, S> {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        self.value.index(index)
    }
}

/**
 * Mul and variants for: borrowed, non-borrowed, and Scalar types
 */

/// Implementation of mul with the beaver trick
/// This implementation panics in the case of a network error.
/// Ideally this is done in a thread where the panic can be handled by the parent.
impl<'a, N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Mul<&'a MpcScalar<N, S>>
    for &'a MpcScalar<N, S>
{
    type Output = MpcScalar<N, S>;

    /// Multiplies two (possibly shared) values. The only case in which we need a Beaver trick
    /// is when both lhs and rhs are Shared. If only one is shared, multiplying by a public value
    /// directly leads to an additive sharing. If both are public, we do not need an additive share.
    /// TODO(@joey): What is the correct behavior when one or both of lhs and rhs are private
    ///
    /// See https://securecomputation.org/docs/pragmaticmpc.pdf (Section 3.4) for the identities this
    /// implementation makes use of.
    fn mul(self, rhs: &'a MpcScalar<N, S>) -> Self::Output {
        if self.is_shared() && rhs.is_shared() {
            let (a, b, c) = self.next_beaver_triplet();

            // Open the value d = [lhs - a].open()
            let lhs_minus_a = (self - &a).open().unwrap();
            // Open the value e = [rhs - b].open()
            let rhs_minus_b = (rhs - &b).open().unwrap();

            // Identity: [a * b] = de + d[b] + e[a] + [c]
            // All multiplications here are between a public and shared value or
            // two public values, so the recursion will not hit this case
            let mut res = &lhs_minus_a * &b + &rhs_minus_b * &a + c;

            // Split into additive shares, the king holds de + res
            if self.network.as_ref().borrow().am_king() {
                res += &lhs_minus_a * &rhs_minus_b;
            }

            res
        } else {
            // Directly multiply
            MpcScalar {
                visibility: Visibility::min_visibility_two(self, rhs),
                network: self.network.clone(),
                beaver_source: self.beaver_source.clone(),
                value: self.value * rhs.value,
            }
        }
    }
}

// Multiplication with a scalar value is equivalent to a public multiplication, no Beaver
// trick needed
macros::impl_arithmetic_assign!(MpcScalar<N, S>, MulAssign, mul_assign, *, Scalar);
macros::impl_arithmetic_assign!(MpcScalar<N, S>, MulAssign, mul_assign, *, MpcScalar<N, S>);
macros::impl_arithmetic_wrapper!(MpcScalar<N, S>, Mul, mul, *, MpcScalar<N, S>);
macros::impl_arithmetic_wrapped!(MpcScalar<N, S>, Mul, mul, *, from_public_scalar, Scalar);

/**
 * Add and variants for: borrowed, non-borrowed, and scalar types
 */
impl<'a, N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Add<&'a MpcScalar<N, S>>
    for &'a MpcScalar<N, S>
{
    type Output = MpcScalar<N, S>;

    fn add(self, rhs: &'a MpcScalar<N, S>) -> Self::Output {
        // If public + shared swap the arguments for simplicity
        if self.is_public() && rhs.is_shared() {
            return rhs + self;
        }

        // If both values are public; both parties add the values together to obtain
        // a public result.
        // If both values are shared; both parties add the shared values together to
        // obtain a shared result.
        // If only one value is public, the king adds the public valid to her share
        // I.e. if the parties hold an additive sharing of a = a_1 + a_2 and with to
        // add public b; the king now holds a_1 + b and the peer holds a_2. Effectively
        // they construct an implicit secret sharing of b where b_1 = b and b_2 = 0
        let am_king = self.network.as_ref().borrow().am_king();

        let res = {
            if self.is_public() && rhs.is_public() ||  // Both public
                self.is_shared() && rhs.is_shared() ||  // Both shared
                am_king
            // One public, but local peer is king
            {
                self.value() + rhs.value()
            } else {
                self.value()
            }
        };

        MpcScalar {
            value: res,
            visibility: Visibility::min_visibility_two(self, rhs),
            network: self.network.clone(),
            beaver_source: self.beaver_source.clone(),
        }
    }
}

macros::impl_arithmetic_assign!(MpcScalar<N, S>, AddAssign, add_assign, +, MpcScalar<N, S>);
macros::impl_arithmetic_assign!(MpcScalar<N, S>, AddAssign, add_assign, +, Scalar);
macros::impl_arithmetic_wrapper!(MpcScalar<N, S>, Add, add, +, MpcScalar<N, S>);
macros::impl_arithmetic_wrapped!(MpcScalar<N, S>, Add, add, +, from_public_scalar, Scalar);

/**
 * Sub and variants for: borrowed, non-borrowed, and scalar types
 */
impl<'a, N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Sub<&'a MpcScalar<N, S>>
    for &'a MpcScalar<N, S>
{
    type Output = MpcScalar<N, S>;

    #[allow(clippy::suspicious_arithmetic_impl)]
    fn sub(self, rhs: &'a MpcScalar<N, S>) -> Self::Output {
        self + rhs.neg()
    }
}

macros::impl_arithmetic_assign!(MpcScalar<N, S>, SubAssign, sub_assign, -, MpcScalar<N, S>);
macros::impl_arithmetic_assign!(MpcScalar<N, S>, SubAssign, sub_assign, -, Scalar);
macros::impl_arithmetic_wrapper!(MpcScalar<N, S>, Sub, sub, -, MpcScalar<N, S>);
macros::impl_arithmetic_wrapped!(MpcScalar<N, S>, Sub, sub, -, from_public_scalar, Scalar);

impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Neg for MpcScalar<N, S> {
    type Output = MpcScalar<N, S>;

    fn neg(self) -> Self::Output {
        (&self).neg()
    }
}

impl<'a, N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Neg for &'a MpcScalar<N, S> {
    type Output = MpcScalar<N, S>;

    fn neg(self) -> Self::Output {
        MpcScalar {
            visibility: self.visibility,
            network: self.network.clone(),
            beaver_source: self.beaver_source.clone(),
            value: self.value.neg(),
        }
    }
}

/**
 * Iterator traits
 */

impl<N, S, T> Product<T> for MpcScalar<N, S>
where
    N: MpcNetwork + Send,
    S: SharedValueSource<Scalar>,
    T: Borrow<MpcScalar<N, S>>,
{
    fn product<I: Iterator<Item = T>>(iter: I) -> Self {
        let mut peekable = iter.peekable();
        let first_elem = peekable.peek().unwrap();
        let network: SharedNetwork<N> = first_elem.borrow().network.clone();
        let beaver_source: BeaverSource<S> = first_elem.borrow().beaver_source.clone();

        peekable.fold(MpcScalar::one(network, beaver_source), |acc, item| {
            acc * item.borrow()
        })
    }
}

impl<N, S, T> Sum<T> for MpcScalar<N, S>
where
    N: MpcNetwork + Send,
    S: SharedValueSource<Scalar>,
    T: Borrow<MpcScalar<N, S>>,
{
    fn sum<I: Iterator<Item = T>>(iter: I) -> Self {
        // This operation is invalid on an empty iterator, unwrap is expected
        let mut peekable = iter.peekable();
        let first_elem = peekable.peek().unwrap();
        let network = first_elem.borrow().network.clone();
        let beaver_source: BeaverSource<S> = first_elem.borrow().beaver_source.clone();

        peekable.fold(
            MpcScalar::from_u64_with_visibility(0, Visibility::Shared, network, beaver_source),
            |acc, item| acc + item.borrow(),
        )
    }
}

impl<N: MpcNetwork + Send, S: SharedValueSource<Scalar>> Zeroize for MpcScalar<N, S> {
    fn zeroize(&mut self) {
        self.value.zeroize()
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use curve25519_dalek::scalar::Scalar;
    use rand_core::OsRng;

    use crate::{beaver::DummySharedScalarSource, network::dummy_network::DummyMpcNetwork};

    use super::{MpcScalar, Visibility};

    #[test]
    fn test_zero() {
        let network = Rc::new(RefCell::new(DummyMpcNetwork::new()));
        let beaver_source = Rc::new(RefCell::new(DummySharedScalarSource::new()));

        let expected =
            MpcScalar::from_public_scalar(Scalar::zero(), network.clone(), beaver_source.clone());
        let zero = MpcScalar::zero(network, beaver_source);

        assert_eq!(zero, expected);
    }

    #[test]
    fn test_open() {
        let network = Rc::new(RefCell::new(DummyMpcNetwork::new()));
        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(1u8)]);

        let beaver_source = Rc::new(RefCell::new(DummySharedScalarSource::new()));

        let expected = MpcScalar::from_public_scalar(
            Scalar::from(2u8),
            network.clone(),
            beaver_source.clone(),
        );

        // Dummy network opens to the value we send it, so the mock parties each hold Scalar(1) for a
        // shared value of Scalar(2)
        let my_share =
            MpcScalar::from_u64_with_visibility(1u64, Visibility::Shared, network, beaver_source);

        assert_eq!(my_share.open().unwrap(), expected);
    }

    #[test]
    fn test_add() {
        let network = Rc::new(RefCell::new(DummyMpcNetwork::new()));
        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(2u8)]);

        let beaver_source = Rc::new(RefCell::new(DummySharedScalarSource::new()));

        // Assume that parties hold a secret share of [4] as individual shares of 2 each
        let shared_value1 = MpcScalar::from_u64_with_visibility(
            2u64,
            Visibility::Shared,
            network.clone(),
            beaver_source.clone(),
        );

        // Test adding a scalar value first
        let res = &shared_value1 + Scalar::from(3u64); // [4] + 3
        assert_eq!(res.visibility, Visibility::Shared);
        assert_eq!(
            res.open().unwrap(),
            MpcScalar::from_public_u64(7u64, network.clone(), beaver_source.clone())
        );

        // Test adding another shared value
        // Assume now that parties have additive shares of [5]
        // The peer holds 1, the local party holds 4
        let shared_value2 = MpcScalar::from_u64_with_visibility(
            4u64,
            Visibility::Shared,
            network.clone(),
            beaver_source.clone(),
        );

        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(3u8)]); // The peer's share of [4] + [5]

        let res = shared_value1 + shared_value2;
        assert_eq!(res.visibility, Visibility::Shared);
        assert_eq!(
            res.open().unwrap(),
            MpcScalar::from_public_u64(9, network, beaver_source)
        )
    }

    #[test]
    fn test_add_associative() {
        let network = Rc::new(RefCell::new(DummyMpcNetwork::new()));
        let beaver_source = Rc::new(RefCell::new(DummySharedScalarSource::new()));

        // Add two random values, ensure associativity
        let mut rng = OsRng {};
        let v1 = MpcScalar::random(&mut rng, network, beaver_source);
        let v2 = Scalar::random(&mut rng);

        let res1 = &v1 + v2;
        let res2 = v2 + &v1;

        assert_eq!(res1, res2);
    }

    #[test]
    fn test_sub() {
        let network = Rc::new(RefCell::new(DummyMpcNetwork::new()));
        let beaver_source = Rc::new(RefCell::new(DummySharedScalarSource::new()));

        // Subtract a raw scalar from a shared value
        // Assume parties hold secret shares 2 and 1 of [3]
        let shared_value1 = MpcScalar::from_u64_with_visibility(
            2u64,
            Visibility::Shared,
            network.clone(),
            beaver_source.clone(),
        );
        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(1u8)]);

        let res = &shared_value1 - Scalar::from(2u8);
        assert_eq!(res.visibility, Visibility::Shared);
        assert_eq!(
            res.open().unwrap(),
            MpcScalar::from_public_u64(1u64, network.clone(), beaver_source.clone())
        );

        // Subtract two shared values
        let shared_value2 = MpcScalar::from_u64_with_visibility(
            5,
            Visibility::Shared,
            network.clone(),
            beaver_source.clone(),
        );
        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(2u8)]);

        let res = shared_value2 - shared_value1;
        assert_eq!(res.visibility, Visibility::Shared);
        assert_eq!(
            res.open().unwrap(),
            MpcScalar::from_public_u64(5, network, beaver_source)
        )
    }

    #[test]
    fn test_mul() {
        let network = Rc::new(RefCell::new(DummyMpcNetwork::new()));
        let beaver_source = Rc::new(RefCell::new(DummySharedScalarSource::new()));

        // Multiply a scalar with a shared value
        // Assume both parties have a sharing of [11], local party holds 6
        let shared_value1 = MpcScalar::from_u64_with_visibility(
            6u64,
            Visibility::Shared,
            network.clone(),
            beaver_source.clone(),
        );

        // Populate the network mock after the multiplication; this implicitly asserts that
        // no network call was used for multiplying by a scalar (assumed public)
        let res = &shared_value1 * Scalar::from(2u8);
        assert_eq!(res.visibility, Visibility::Shared);

        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(10u8)]);

        assert_eq!(
            res.open().unwrap(),
            MpcScalar::from_public_u64(22, network.clone(), beaver_source.clone())
        );

        // Multiply a shared value with a public value
        let public_value = MpcScalar::from_public_u64(3u64, network.clone(), beaver_source.clone());

        // As above, populate the network mock after the multiplication
        let res = public_value * &shared_value1;
        assert_eq!(res.visibility, Visibility::Shared);

        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(15u8)]);
        assert_eq!(
            res.open().unwrap(),
            MpcScalar::from_public_u64(33u64, network.clone(), beaver_source.clone())
        );

        // Multiply two shared values, a beaver triplet (a, b, c) will be needed
        // Populate the network mock with two openings:
        //      1. [shared1 - a]
        //      2. [shared2 - b]
        // Assume that the parties hold [shared2] = [12] where the peer holds 7 and the local holds 5
        let shared_value2 = MpcScalar::from_u64_with_visibility(
            5u64,
            Visibility::Shared,
            network.clone(),
            beaver_source.clone(),
        );
        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(5u8), Scalar::from(7u8)]);

        // Populate the network with the peer's res share after the computation
        let res = shared_value1 * shared_value2;
        assert_eq!(res.visibility, Visibility::Shared);

        network
            .borrow_mut()
            .add_mock_scalars(vec![Scalar::from(0u64)]);

        assert_eq!(
            res.open().unwrap(),
            MpcScalar::from_public_u64(12 * 11, network, beaver_source)
        )
    }
}
