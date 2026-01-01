// fixed_str/src/serialize_ext.rs

//******************************************************************************
//  BinRW Serialization
//******************************************************************************

#[cfg(feature = "binrw")]
mod binrw_ext {
    use crate::*;
    use binrw::io::{Read, Seek, Write};
    use binrw::{BinRead, BinWrite};

    /// Implements binary reading for `FixedStr` using the binrw crate.
    impl<const N: usize> BinRead for FixedStr<N> {
        type Args<'a> = ();

        fn read_options<R: Read + Seek>(
            reader: &mut R,
            _endian: binrw::Endian,
            _args: Self::Args<'_>,
        ) -> binrw::BinResult<Self> {
            let mut buf = [0u8; N];
            reader.read_exact(&mut buf)?;
            Ok(Self { data: buf })
        }
    }

    /// Implements binary writing for `FixedStr` using the binrw crate.
    impl<const N: usize> BinWrite for FixedStr<N> {
        type Args<'a> = ();

        fn write_options<W: Write + Seek>(
            &self,
            writer: &mut W,
            _endian: binrw::Endian,
            _args: Self::Args<'_>,
        ) -> binrw::BinResult<()> {
            writer.write_all(&self.data)?;
            Ok(())
        }
    }
}

// --- Tests for binrw integration ---
#[cfg(all(test, feature = "binrw", feature = "std"))]
mod binrw_tests {
    use crate::*;

    #[test]
    fn test_binrw_roundtrip() {
        use binrw::{BinRead, BinWrite, Endian};
        use std::io::Cursor;

        let original = FixedStr::<5>::new("Hello");
        // Use a Cursor for both writing and reading.
        let mut cursor = Cursor::new(Vec::new());
        original
            .write_options(&mut cursor, Endian::Little, ())
            .expect("writing failed");
        cursor.set_position(0);
        let read: FixedStr<5> =
            FixedStr::read_options(&mut cursor, Endian::Little, ()).expect("reading failed");
        assert_eq!(original, read);
    }
}

//******************************************************************************
//  rkyv Serialization
//******************************************************************************

#[cfg(feature = "rkyv")]
mod rkyv_ext {
    use crate::*;
    use rkyv::bytecheck::{rancor::Fallible as BytecheckFallible, CheckBytes};
    use rkyv::rancor::Fallible;
    use rkyv::ser::Writer;
    use rkyv::traits::CopyOptimization;
    use rkyv::validation::ArchiveContext;
    use rkyv::{Archive, Deserialize, Portable, Serialize};

    /// Declares that `FixedStr` is portable across architectures.
    unsafe impl<const N: usize> Portable for FixedStr<N> {}

    /// Implements bytecheck validation for `FixedStr`.
    unsafe impl<const N: usize, C> CheckBytes<C> for FixedStr<N>
    where
        C: BytecheckFallible + ArchiveContext + ?Sized,
    {
        unsafe fn check_bytes(_value: *const Self, _context: &mut C) -> Result<(), C::Error> {
            // FixedStr is just a transparent wrapper around [u8; N], so it's always valid
            Ok(())
        }
    }

    /// Implements rkyv archiving for `FixedStr`.
    /// The archived form is `FixedStr` itself.
    impl<const N: usize> Archive for FixedStr<N> {
        type Archived = Self;
        type Resolver = ();

        /// Enables copy optimization for efficient serialization.
        const COPY_OPTIMIZATION: CopyOptimization<Self> = unsafe { CopyOptimization::enable() };

        #[inline]
        fn resolve(&self, _resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
            // SAFETY: FixedStr is Copy and repr(transparent) around [u8; N]
            unsafe {
                core::ptr::write(out.ptr() as *mut Self, *self);
            }
        }
    }

    /// Implements rkyv serialization for `FixedStr`.
    impl<const N: usize, S> Serialize<S> for FixedStr<N>
    where
        S: Fallible + Writer + ?Sized,
    {
        fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
            serializer.write(&self.data)
        }
    }

    /// Implements rkyv deserialization for `FixedStr`.
    impl<const N: usize, D> Deserialize<FixedStr<N>, D> for FixedStr<N>
    where
        D: Fallible + ?Sized,
    {
        fn deserialize(&self, _deserializer: &mut D) -> Result<FixedStr<N>, D::Error> {
            Ok(*self)
        }
    }
}

// --- Tests for rkyv integration ---
#[cfg(all(test, feature = "rkyv", feature = "std"))]
mod rkyv_tests {
    use crate::*;
    use rkyv::{access, access_unchecked, deserialize, to_bytes};

    #[test]
    fn test_rkyv_roundtrip() {
        let original = FixedStr::<10>::new("Hello");

        // Serialize to bytes.
        let bytes = to_bytes::<rkyv::rancor::Error>(&original).expect("serialization failed");

        // Access archived data (zero-copy). The archived form is FixedStr<10>.
        let archived =
            access::<FixedStr<10>, rkyv::rancor::Error>(&bytes[..]).expect("access failed");

        // Deserialize back to original type.
        let deserialized = deserialize::<FixedStr<10>, rkyv::rancor::Error>(archived)
            .expect("deserialization failed");

        assert_eq!(deserialized, original);
    }

    #[test]
    fn test_rkyv_zero_copy() {
        let original = FixedStr::<8>::new("Test123");
        let bytes = to_bytes::<rkyv::rancor::Error>(&original).expect("serialization failed");

        // Zero-copy access without deserialization.
        let archived = unsafe { access_unchecked::<FixedStr<8>>(&bytes[..]) };
        assert_eq!(archived.as_str(), "Test123");
    }
}

//******************************************************************************
//  Serde Serialization
//******************************************************************************

#[cfg(feature = "serde")]
mod serde_ext {
    use crate::*;
    use core::fmt;
    use serde::de::{Error as DeError, Visitor};
    use serde::ser::Error as SerError;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Implements Serde serialization for `FixedStr`.
    impl<const N: usize> Serialize for FixedStr<N> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match self.try_as_str() {
                Ok(s) => serializer.serialize_str(s),
                Err(_) => Err(S::Error::custom(FixedStrError::InvalidUtf8)),
            }
        }
    }

    /// A visitor for deserializing a `FixedStr`.
    struct FixedStrVisitor<const N: usize>;

    impl<const N: usize> Visitor<'_> for FixedStrVisitor<N> {
        type Value = FixedStr<N>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a string of at most {} bytes", N)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: DeError,
        {
            Ok(FixedStr::new(value))
        }
    }

    /// Implements Serde deserialization for `FixedStr`.
    impl<'de, const N: usize> Deserialize<'de> for FixedStr<N> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_str(FixedStrVisitor::<N>)
        }
    }
}

/// Provides alternative (byte-based) serialization for `FixedStr` via Serde.
#[cfg(feature = "serde")]
pub mod serde_as_bytes {
    use crate::FixedStr;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializes a `FixedStr<N>` as raw bytes.
    pub fn serialize<S, const N: usize>(
        value: &FixedStr<N>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(value.as_bytes())
    }

    /// Deserializes a `FixedStr<N>` from raw bytes.
    pub fn deserialize<'de, D, const N: usize>(deserializer: D) -> Result<FixedStr<N>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: &[u8] = Deserialize::deserialize(deserializer)?;
        FixedStr::<N>::try_from(bytes).map_err(serde::de::Error::custom)
    }
}

// --- Tests for Serde integration ---
#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use crate::*;
    use serde::{Deserialize, Serialize};
    use serde_test::{assert_tokens, Token};

    /// A test structure to verify byte-based serialization of FixedStr.
    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ByteWrapper {
        #[serde(with = "serialize_ext::serde_as_bytes")]
        inner: FixedStr<5>,
    }

    #[test]
    fn test_serde_as_bytes() {
        let wrapper = ByteWrapper {
            inner: FixedStr::new("Hello"),
        };

        // Tokens for a struct with a named field.
        assert_tokens(
            &wrapper,
            &[
                Token::Struct {
                    name: "ByteWrapper",
                    len: 1,
                },
                Token::Str("inner"),
                Token::BorrowedBytes(b"Hello"),
                Token::StructEnd,
            ],
        );
    }
}
