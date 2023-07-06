//! Defines the `Scalar` type of the Starknet field

use std::{
    iter::Sum,
    mem::size_of,
    ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign},
};

use ark_ec::{
    short_weierstrass::{Affine, Projective, SWCurveConfig},
    CurveConfig, CurveGroup, Group, VariableBaseMSM,
};
use ark_ff::{MontFp, Zero};

use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, SerializationError};
use itertools::Itertools;
use serde::{de::Error as DeError, Deserialize, Serialize};

use crate::fabric::{cast_args, ResultHandle, ResultValue};

use super::{
    macros::{impl_borrow_variants, impl_commutative},
    scalar::{Scalar, ScalarInner, ScalarResult, StarknetBaseFelt},
};

/// The number of points and scalars to pull from an iterated MSM when
/// performing a multiscalar multiplication
const MSM_CHUNK_SIZE: usize = 1 << 16;

/// The Stark curve in the arkworks short Weierstrass curve representation
pub struct StarknetCurveConfig;
impl CurveConfig for StarknetCurveConfig {
    type BaseField = StarknetBaseFelt;
    type ScalarField = ScalarInner;

    const COFACTOR: &'static [u64] = &[1];
    const COFACTOR_INV: Self::ScalarField = MontFp!("1");
}

/// See https://docs.starkware.co/starkex/crypto/stark-curve.html
/// for curve parameters
impl SWCurveConfig for StarknetCurveConfig {
    const COEFF_A: Self::BaseField = MontFp!("1");
    const COEFF_B: Self::BaseField =
        MontFp!("3141592653589793238462643383279502884197169399375105820974944592307816406665");

    const GENERATOR: Affine<Self> = Affine {
        x: MontFp!("874739451078007766457464989774322083649278607533249481151382481072868806602"),
        y: MontFp!("152666792071518830868575557812948353041420400780739481342941381225525861407"),
        infinity: false,
    };
}

/// A type alias for a projective curve point on the Stark curve
pub(crate) type StarkPointInner = Projective<StarknetCurveConfig>;
/// A wrapper around the inner point that allows us to define foreign traits on the point
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StarkPoint(pub(crate) StarkPointInner);

impl Serialize for StarkPoint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let bytes = self.to_bytes();
        bytes.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StarkPoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        StarkPoint::from_bytes(&bytes)
            .map_err(|err| DeError::custom(format!("Failed to deserialize point: {err:?}")))
    }
}

// ------------------------
// | Misc Implementations |
// ------------------------

impl StarkPoint {
    /// The additive identity in the curve group
    pub fn zero() -> StarkPoint {
        StarkPoint(StarkPointInner::zero())
    }

    /// The group generator
    pub fn generator() -> StarkPoint {
        StarkPoint(StarkPointInner::generator())
    }

    /// Serialize this point to a byte buffer
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::with_capacity(size_of::<StarkPoint>());
        self.0
            .serialize_uncompressed(&mut out)
            .expect("Failed to serialize point");

        out
    }

    /// Deserialize a point from a byte buffer
    pub fn from_bytes(bytes: &[u8]) -> Result<StarkPoint, SerializationError> {
        let point = StarkPointInner::deserialize_uncompressed(bytes)?;
        Ok(StarkPoint(point))
    }
}

// ------------------------------------
// | Curve Arithmetic Implementations |
// ------------------------------------

// === Addition === //

impl Add<&StarkPointInner> for &StarkPoint {
    type Output = StarkPoint;

    fn add(self, rhs: &StarkPointInner) -> Self::Output {
        StarkPoint(self.0 + rhs)
    }
}
impl_borrow_variants!(StarkPoint, Add, add, +, StarkPointInner);
impl_commutative!(StarkPoint, Add, add, +, StarkPointInner);

impl Add<&StarkPoint> for &StarkPoint {
    type Output = StarkPoint;

    fn add(self, rhs: &StarkPoint) -> Self::Output {
        StarkPoint(self.0 + rhs.0)
    }
}
impl_borrow_variants!(StarkPoint, Add, add, +, StarkPoint);

/// A type alias for a result that resolves to a `StarkPoint`
pub type StarkPointResult = ResultHandle<StarkPoint>;

impl Add<&StarkPointResult> for &StarkPointResult {
    type Output = StarkPointResult;

    fn add(self, rhs: &StarkPointResult) -> Self::Output {
        self.fabric.new_gate_op(vec![self.id, rhs.id], |args| {
            let [lhs, rhs]: [StarkPoint; 2] = cast_args(args);
            ResultValue::Point(StarkPoint(lhs.0 + rhs.0))
        })
    }
}
impl_borrow_variants!(StarkPointResult, Add, add, +, StarkPointResult);

impl Add<&StarkPoint> for &StarkPointResult {
    type Output = StarkPointResult;

    fn add(self, rhs: &StarkPoint) -> Self::Output {
        let rhs = *rhs;
        self.fabric.new_gate_op(vec![self.id], move |args| {
            let [lhs]: [StarkPoint; 1] = cast_args(args);
            ResultValue::Point(StarkPoint(lhs.0 + rhs.0))
        })
    }
}
impl_borrow_variants!(StarkPointResult, Add, add, +, StarkPoint);
impl_commutative!(StarkPointResult, Add, add, +, StarkPoint);

// === AddAssign === //

impl AddAssign for StarkPoint {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

// === Subtraction === //

impl Sub<&StarkPoint> for &StarkPoint {
    type Output = StarkPoint;

    fn sub(self, rhs: &StarkPoint) -> Self::Output {
        StarkPoint(self.0 - rhs.0)
    }
}
impl_borrow_variants!(StarkPoint, Sub, sub, -, StarkPoint);

impl Sub<&StarkPointResult> for &StarkPointResult {
    type Output = StarkPointResult;

    fn sub(self, rhs: &StarkPointResult) -> Self::Output {
        self.fabric.new_gate_op(vec![self.id, rhs.id], |args| {
            let [lhs, rhs]: [StarkPoint; 2] = cast_args(args);
            ResultValue::Point(StarkPoint(lhs.0 - rhs.0))
        })
    }
}
impl_borrow_variants!(StarkPointResult, Sub, sub, -, StarkPointResult);

impl Sub<&StarkPoint> for &StarkPointResult {
    type Output = StarkPointResult;

    fn sub(self, rhs: &StarkPoint) -> Self::Output {
        let rhs = *rhs;
        self.fabric.new_gate_op(vec![self.id], move |args| {
            let [lhs]: [StarkPoint; 1] = cast_args(args);
            ResultValue::Point(StarkPoint(lhs.0 - rhs.0))
        })
    }
}
impl_borrow_variants!(StarkPointResult, Sub, sub, -, StarkPoint);

impl Sub<&StarkPointResult> for &StarkPoint {
    type Output = StarkPointResult;

    fn sub(self, rhs: &StarkPointResult) -> Self::Output {
        let self_owned = *self;
        rhs.fabric.new_gate_op(vec![rhs.id], move |args| {
            let [rhs]: [StarkPoint; 1] = cast_args(args);
            ResultValue::Point(StarkPoint(self_owned.0 - rhs.0))
        })
    }
}

// === SubAssign === //

impl SubAssign for StarkPoint {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

// === Negation === //

impl Neg for &StarkPoint {
    type Output = StarkPoint;

    fn neg(self) -> Self::Output {
        StarkPoint(-self.0)
    }
}
impl_borrow_variants!(StarkPoint, Neg, neg, -);

impl Neg for &StarkPointResult {
    type Output = StarkPointResult;

    fn neg(self) -> Self::Output {
        self.fabric.new_gate_op(vec![self.id], |args| {
            let [lhs]: [StarkPoint; 1] = cast_args(args);
            ResultValue::Point(StarkPoint(-lhs.0))
        })
    }
}
impl_borrow_variants!(StarkPointResult, Neg, neg, -);

// === Scalar Multiplication === //

impl Mul<&Scalar> for &StarkPoint {
    type Output = StarkPoint;

    fn mul(self, rhs: &Scalar) -> Self::Output {
        StarkPoint(self.0 * rhs.0)
    }
}
impl_borrow_variants!(StarkPoint, Mul, mul, *, Scalar);
impl_commutative!(StarkPoint, Mul, mul, *, Scalar);

impl Mul<&Scalar> for &StarkPointResult {
    type Output = StarkPointResult;

    fn mul(self, rhs: &Scalar) -> Self::Output {
        let rhs = *rhs;
        self.fabric.new_gate_op(vec![self.id], move |args| {
            let [lhs]: [StarkPoint; 1] = cast_args(args);
            ResultValue::Point(StarkPoint(lhs.0 * rhs.0))
        })
    }
}
impl_borrow_variants!(StarkPointResult, Mul, mul, *, Scalar);
impl_commutative!(StarkPointResult, Mul, mul, *, Scalar);

impl Mul<&ScalarResult> for &StarkPoint {
    type Output = StarkPointResult;

    fn mul(self, rhs: &ScalarResult) -> Self::Output {
        let self_owned = *self;
        rhs.fabric.new_gate_op(vec![rhs.id], move |args| {
            let [rhs]: [Scalar; 1] = cast_args(args);
            ResultValue::Point(StarkPoint(self_owned.0 * rhs.0))
        })
    }
}
impl_borrow_variants!(StarkPoint, Mul, mul, *, ScalarResult, Output=StarkPointResult);
impl_commutative!(StarkPoint, Mul, mul, *, ScalarResult, Output=StarkPointResult);

impl Mul<&ScalarResult> for &StarkPointResult {
    type Output = StarkPointResult;

    fn mul(self, rhs: &ScalarResult) -> Self::Output {
        self.fabric.new_gate_op(vec![self.id, rhs.id], |mut args| {
            let lhs: StarkPoint = args.remove(0).into();
            let rhs: Scalar = args.remove(0).into();

            ResultValue::Point(StarkPoint(lhs.0 * rhs.0))
        })
    }
}
impl_borrow_variants!(StarkPointResult, Mul, mul, *, ScalarResult);
impl_commutative!(StarkPointResult, Mul, mul, *, ScalarResult);

// === MulAssign === //

impl MulAssign<&Scalar> for StarkPoint {
    fn mul_assign(&mut self, rhs: &Scalar) {
        self.0 *= rhs.0;
    }
}

// -------------------
// | Iterator Traits |
// -------------------

impl Sum for StarkPoint {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(StarkPoint::zero(), |acc, x| acc + x)
    }
}

/// MSM Implementation
impl StarkPoint {
    /// Compute the multiscalar multiplication of the given scalars and points
    pub fn msm(scalars: &[Scalar], points: &[StarkPoint]) -> StarkPoint {
        assert_eq!(
            scalars.len(),
            points.len(),
            "msm cannot compute on vectors of unequal length"
        );

        let affine_points = points.iter().map(|p| p.0.into_affine()).collect_vec();
        let stripped_scalars = scalars.iter().map(|s| s.0).collect_vec();
        StarkPointInner::msm(&affine_points, &stripped_scalars)
            .map(StarkPoint)
            .unwrap()
    }

    /// Compute the multiscalar multiplication of the given scalars and points
    /// represented as streaming iterators
    ///
    /// This is roughly a re-implementation of the `ark-ec` msm defined here:
    ///     https://github.com/arkworks-rs/algebra/blob/master/ec/src/scalar_mul/variable_base/mod.rs#L54-L60
    /// but with less restrictive trait bounds
    pub fn msm_iter<I, J>(scalars: I, points: J) -> StarkPoint
    where
        I: IntoIterator<Item = Scalar>,
        J: IntoIterator<Item = StarkPoint>,
    {
        let scalars = scalars.into_iter().map(|s| s.0).chunks(MSM_CHUNK_SIZE);
        let points = points
            .into_iter()
            .map(|p| p.0.into_affine())
            .chunks(MSM_CHUNK_SIZE);

        let mut res = StarkPointInner::zero();
        for (scalar_chunk, point_chunk) in scalars.into_iter().zip(points.into_iter()) {
            let scalar_chunk: Vec<ScalarInner> = scalar_chunk.collect();
            let point_chunk: Vec<Affine<StarknetCurveConfig>> = point_chunk.collect();

            let chunk_res = StarkPointInner::msm_unchecked(&point_chunk, &scalar_chunk);

            res += chunk_res;
        }

        StarkPoint(res)
    }
}

// ---------
// | Tests |
// ---------

/// We test our config against a known implementation of the Stark curve:
///     https://github.com/xJonathanLEI/starknet-rs
#[cfg(test)]
mod test {
    use starknet_curve::{curve_params::GENERATOR, ProjectivePoint};

    use crate::{
        algebra::test_helper::{
            arkworks_point_to_starknet, compare_points, prime_field_to_starknet_felt, random_point,
            starknet_rs_scalar_mul,
        },
        random_scalar,
    };

    use super::*;
    /// Test that the generators are the same between the two curve representations
    #[test]
    fn test_generators() {
        let generator_1 = StarkPoint::generator();
        let generator_2 = ProjectivePoint::from_affine_point(&GENERATOR);

        assert!(compare_points(&generator_1, &generator_2));
    }

    /// Tests point addition
    #[test]
    fn test_point_addition() {
        let p1 = random_point();
        let q1 = random_point();

        let p2 = arkworks_point_to_starknet(&p1);
        let q2 = arkworks_point_to_starknet(&q1);

        let r1 = p1 + q1;

        // Only `AddAssign` is implemented on `ProjectivePoint`
        let mut r2 = p2;
        r2 += &q2;

        assert!(compare_points(&r1, &r2));
    }

    /// Tests scalar multiplication
    #[test]
    fn test_scalar_mul() {
        let s1 = random_scalar();
        let p1 = random_point();

        let s2 = prime_field_to_starknet_felt(&s1.0);
        let p2 = arkworks_point_to_starknet(&p1);

        let r1 = p1 * s1;
        let r2 = starknet_rs_scalar_mul(&s2, &p2);

        assert!(compare_points(&r1, &r2));
    }

    /// Tests addition with the additive identity
    #[test]
    fn test_additive_identity() {
        let p1 = random_point();
        let res = p1 + StarkPoint::zero();

        assert_eq!(p1, res);
    }
}
