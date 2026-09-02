use crate::CanonicalEncode;
use crate::CanonicalHasher;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }

            pub fn parse(value: &str) -> Result<Self, uuid::Error> {
                Uuid::parse_str(value).map(Self)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl CanonicalEncode for $name {
            fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
                hasher.bytes(self.0.as_bytes());
            }
        }
    };
}

uuid_id!(BatchId);
uuid_id!(CommandId);
uuid_id!(EventId);
uuid_id!(ItemId);
uuid_id!(ThreadId);
uuid_id!(TurnId);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReceiptId(Uuid);

impl ReceiptId {
    const NAMESPACE: Uuid = Uuid::from_u128(0x43d5_e6b6_7610_5c89_a255_6730_20ce_d910);

    pub fn for_command(command_id: CommandId) -> Self {
        Self(Uuid::new_v5(
            &Self::NAMESPACE,
            command_id.as_uuid().as_bytes(),
        ))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ReceiptId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl CanonicalEncode for ReceiptId {
    fn encode_canonical(&self, hasher: &mut CanonicalHasher) {
        hasher.bytes(self.0.as_bytes());
    }
}
