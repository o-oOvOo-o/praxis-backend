use super::ids::ProductProfileId;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ProductProfile {
    pub(crate) id: ProductProfileId,
    pub(crate) name: &'static str,
}

pub(crate) fn praxis_product_profile() -> ProductProfile {
    ProductProfile {
        id: ProductProfileId::Praxis,
        name: "Praxis",
    }
}
